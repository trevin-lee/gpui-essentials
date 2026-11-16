//! Flows from elsewhere: subcanvas's whiteboard JSON, and Mermaid flowcharts.
//!
//! Each importer turns its text into a [`FlowObject`] to give [`crate::Flow::restore`] (or to
//! add to a flow). Nothing in the text can make the flow fetch or open anything: links in it
//! become ids and addresses for the host, and Mermaid's `click` lines are left out. Input is
//! read defensively and within limits, so a hostile file loads as what it can, or fails with
//! an error, and never panics or runs on.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde_json::{Map, Value, json};

use crate::shapes::NodeShape;
use crate::types::*;

/// Why a file couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportError(pub String);

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImportError {}

/// The most nodes and edges an import reads; the rest are left out.
pub const MAX_NODES: usize = 2000;
pub const MAX_EDGES: usize = 5000;
/// The longest text kept in a label or a title.
const MAX_TEXT: usize = 2000;

fn text(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?.trim();
    (!s.is_empty()).then(|| s.chars().take(MAX_TEXT).collect())
}

fn number(v: Option<&Value>) -> Option<f32> {
    v?.as_f64().map(|n| n as f32).filter(|n| n.is_finite())
}

fn first<'a>(o: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| o.get(*k).filter(|v| !v.is_null()))
}

// subcanvas.

/// A subcanvas whiteboard as its `read_whiteboard` tool gives it (or as its own document holds
/// it, with camelCase names): `nodes` and `edges` at the top.
///
/// - `plain` nodes are shapes, `text` nodes headings, `group`s frames holding the nodes whose
///   `group_id` (`parentId`) they are, `media` nodes pictures (their `url` or `mediaPath` the
///   `src` the host's loader is asked for), `link` nodes link cards, and drawings drawings.
/// - Colors, shapes, icons and emoji keep their names; a node's or an edge's document
///   (`doc_id`) is its link.
/// - Edges keep their label, color and dots, `direction` decides their arrowheads, and
///   `step` edges are smooth steps. One with no handles meets its nodes on the sides that face
///   each other.
pub fn from_subcanvas(json: &str) -> Result<FlowObject, ImportError> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| ImportError(format!("not JSON: {e}")))?;
    // Either the whiteboard itself, or a tool result with it under `data`.
    let board = if root.get("nodes").is_some() {
        &root
    } else {
        root.get("data").unwrap_or(&root)
    };
    let nodes = board
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| ImportError("no nodes".into()))?;
    let mut out = Vec::new();
    let mut ids = HashSet::new();
    for n in nodes.iter().take(MAX_NODES) {
        let Some(o) = n.as_object() else { continue };
        let Some(id) = text(o.get("id")) else {
            continue;
        };
        if !ids.insert(id.clone()) {
            continue;
        }
        out.push(subcanvas_node(id, o));
    }
    // Parents that aren't there are dropped.
    for n in &mut out {
        if n.parent.as_ref().is_some_and(|p| !ids.contains(p.as_ref())) {
            n.parent = None;
        }
    }
    let mut edges = Vec::new();
    let mut edge_ids = HashSet::new();
    for e in board
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_EDGES)
    {
        let Some(o) = e.as_object() else { continue };
        let (Some(id), Some(source), Some(target)) = (
            text(o.get("id")),
            text(o.get("source")),
            text(o.get("target")),
        ) else {
            continue;
        };
        if !ids.contains(&source) || !ids.contains(&target) || !edge_ids.insert(id.clone()) {
            continue;
        }
        edges.push(subcanvas_edge(id, source, target, o, &out));
    }
    Ok(FlowObject {
        nodes: out,
        edges,
        viewport: Viewport::default(),
    })
}

fn subcanvas_node(id: String, o: &Map<String, Value>) -> Node {
    let kind = text(o.get("kind")).unwrap_or_else(|| "plain".into());
    let title = text(o.get("title")).unwrap_or_default();
    let at = xy(
        number(o.get("x")).unwrap_or(0.),
        number(o.get("y")).unwrap_or(0.),
    );
    let mut node = Node::new(id, at).label(title.clone());
    match kind.as_str() {
        "text" => node.kind = "text".into(),
        "group" => node.kind = "frame".into(),
        "media" => {
            let media = o.get("media").and_then(Value::as_object);
            let pick = |keys: &[&str]| {
                media
                    .and_then(|m| first(m, keys))
                    .or_else(|| first(o, keys))
            };
            node.kind = "image".into();
            node.media = Some(Media {
                src: text(pick(&["url", "mediaPath", "media_path", "src"]))
                    .unwrap_or_default()
                    .into(),
                width: number(pick(&["width", "mediaWidth"])),
                height: number(pick(&["height", "mediaHeight"])),
                alt: text(pick(&["alt"])).unwrap_or_default().into(),
                extra: Map::new(),
            });
        }
        "link" => {
            node.kind = "link".into();
            node.card = Some(LinkCard {
                url: text(first(o, &["url", "href"])).unwrap_or_default().into(),
                title: (!title.is_empty()).then(|| title.clone().into()),
                description: text(o.get("description")).map(Into::into),
                ..LinkCard::default()
            });
        }
        "drawing" => {
            node.kind = "drawing".into();
            if let Some(points) = text(o.get("points")) {
                node.data["points"] = Value::String(points);
            }
        }
        _ => {
            node.kind = "shape".into();
            node.shape = Some(
                text(o.get("shape"))
                    .and_then(|s| serde_json::from_value(Value::String(s)).ok())
                    .unwrap_or(NodeShape::Rectangle),
            );
        }
    }
    let (w, h) = (
        number(o.get("width")).filter(|v| *v > 0.),
        number(o.get("height")).filter(|v| *v > 0.),
    );
    node.width = w;
    node.height = h;
    // A shape without a size takes its shape's; a frame, subcanvas's group size.
    if node.kind == "shape" && (w.is_none() || h.is_none()) {
        let (dw, dh) = node.shape.unwrap_or_default().default_size();
        node.width = Some(w.unwrap_or(dw));
        node.height = Some(h.unwrap_or(dh));
    }
    if node.kind == "frame" {
        node.width = Some(w.unwrap_or(360.));
        node.height = Some(h.unwrap_or(240.));
    }
    node.parent = text(first(o, &["group_id", "parentId", "parent_id"])).map(Into::into);
    node.color = text(o.get("color"))
        .filter(|c| c != "default")
        .map(Into::into);
    node.icon = text(o.get("icon")).map(Into::into);
    node.emoji = text(o.get("emoji")).map(Into::into);
    node.link = text(first(o, &["doc_id", "docId"])).map(Into::into);
    if let Some(d) = text(o.get("description")) {
        node.data["description"] = Value::String(d);
    }
    for key in [
        "doc_type",
        "docType",
        "open_mode",
        "openMode",
        "repository_path",
        "path",
    ] {
        if let Some(v) = o.get(key).filter(|v| !v.is_null()) {
            node.extra.insert(key.into(), v.clone());
        }
    }
    node
}

fn subcanvas_edge(
    id: String,
    source: String,
    target: String,
    o: &Map<String, Value>,
    nodes: &[Node],
) -> Edge {
    let mut edge = Edge::new(id, source, target);
    edge.label = text(o.get("label")).map(Into::into);
    edge.kind = match text(o.get("shape")).as_deref() {
        Some("step") => EdgeKind::SmoothStep,
        _ => EdgeKind::Bezier,
    };
    edge.dash = match text(o.get("stroke")).as_deref() {
        Some("dotted") => EdgeDash::Dotted,
        Some("dashed") => EdgeDash::Dashed,
        _ => EdgeDash::Solid,
    };
    edge.pencil = text(o.get("color"))
        .filter(|c| c != "default")
        .map(Into::into);
    edge = edge.arrows(match text(o.get("direction")).as_deref() {
        Some("none") => Arrows::None,
        Some("reverse") => Arrows::Reverse,
        Some("both") => Arrows::Both,
        _ => Arrows::Forward,
    });
    if let Some(doc) = text(first(o, &["doc_id", "docId"])) {
        edge.data = json!({ "docId": doc });
    }
    // Handles, as subcanvas names them (its sides), or else the sides that face each other.
    let side =
        |v: Option<String>| v.filter(|s| ["top", "right", "bottom", "left"].contains(&s.as_str()));
    let (sh, th) = (
        side(text(first(o, &["sourceHandle", "source_handle"]))),
        side(text(first(o, &["targetHandle", "target_handle"]))),
    );
    let facing = facing_sides(nodes, &edge.source, &edge.target);
    let boxed = |id: &NodeId| {
        nodes
            .iter()
            .find(|n| &n.id == id)
            .is_some_and(|n| matches!(n.kind.as_ref(), "shape" | "link"))
    };
    if boxed(&edge.source) {
        edge.source_handle = sh.or(facing.map(|f| f.0.into())).map(Into::into);
    }
    if boxed(&edge.target) {
        edge.target_handle = th.or(facing.map(|f| f.1.into())).map(Into::into);
    }
    edge
}

/// The sides of two nodes that face each other, by where their middles are (in the flow,
/// whatever group each is in).
fn facing_sides(nodes: &[Node], a: &NodeId, b: &NodeId) -> Option<(&'static str, &'static str)> {
    let middle = |id: &NodeId| {
        let mut n = nodes.iter().find(|n| &n.id == id)?;
        let mut at = n.position;
        let size = xy(n.width.unwrap_or(160.), n.height.unwrap_or(64.));
        for _ in 0..32 {
            let Some(p) = n
                .parent
                .as_ref()
                .and_then(|p| nodes.iter().find(|m| &m.id == p))
            else {
                break;
            };
            at = at + p.position;
            n = p;
        }
        Some(at + size * 0.5)
    };
    let d = middle(b)? - middle(a)?;
    Some(if d.x.abs() >= d.y.abs() {
        if d.x >= 0. {
            ("right", "left")
        } else {
            ("left", "right")
        }
    } else if d.y >= 0. {
        ("bottom", "top")
    } else {
        ("top", "bottom")
    })
}

// Mermaid.

/// A Mermaid flowchart (`flowchart` or `graph`, any direction), laid out in layers the way
/// Mermaid draws it.
///
/// - Nodes keep their shapes: rectangle, rounded and stadium, circle, cylinder, diamond,
///   hexagon, parallelogram and the rest Mermaid has, mapped to the nearest whiteboard shape.
/// - Arrows keep their labels (`-- text -->` and `-->|text|`), dots (`-.->`), weight (`==>`)
///   and heads (`<-->`, `---`); chains (`a --> b --> c`) and `&` are read.
/// - `subgraph` becomes a frame round its nodes.
/// - `click`, `style`, `classDef` and `class` lines are left out: a diagram never makes the
///   flow open anything.
pub fn from_mermaid(src: &str) -> Result<FlowObject, ImportError> {
    let mut chart = Chart::default();
    let mut lines = src.lines().peekable();
    // The header: `flowchart TD`, `graph LR`, and so on, after any front matter or comments.
    let mut header = None;
    let mut front_matter = false;
    while let Some(line) = lines.peek() {
        let l = line.trim();
        if l == "---" {
            front_matter = !front_matter;
            lines.next();
            continue;
        }
        if front_matter || l.is_empty() || l.starts_with("%%") {
            lines.next();
            continue;
        }
        header = Some(l.to_string());
        lines.next();
        break;
    }
    let header = header.ok_or_else(|| ImportError("an empty diagram".into()))?;
    let mut words = header.split_whitespace();
    match words.next() {
        Some("flowchart" | "graph") => {}
        Some(other) => {
            return Err(ImportError(format!(
                "a `{other}` diagram: only flowcharts are read"
            )));
        }
        None => return Err(ImportError("an empty diagram".into())),
    }
    chart.direction = match words.next().map(|w| w.trim_end_matches(';')) {
        Some("LR") => Dir::Right,
        Some("RL") => Dir::Left,
        Some("BT") => Dir::Up,
        _ => Dir::Down,
    };
    for line in lines {
        for statement in split_statements(line) {
            chart.statement(statement.trim());
        }
    }
    Ok(chart.into_flow())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Dir {
    #[default]
    Down,
    Up,
    Right,
    Left,
}

#[derive(Default)]
struct Chart {
    direction: Dir,
    /// In the order they were first named.
    nodes: Vec<(String, Option<String>, NodeShape)>,
    index: HashMap<String, usize>,
    edges: Vec<ChartEdge>,
    /// Open subgraphs, innermost last: (id, title).
    open: Vec<(String, String)>,
    /// Every subgraph: (id, title, parent subgraph).
    subgraphs: Vec<(String, String, Option<String>)>,
    /// Which subgraph each node was first named in.
    member: HashMap<String, String>,
}

struct ChartEdge {
    from: String,
    to: String,
    label: Option<String>,
    dash: EdgeDash,
    thick: bool,
    arrows: Arrows,
}

/// A line's statements: split at semicolons outside quotes and brackets.
fn split_statements(line: &str) -> Vec<&str> {
    let (mut out, mut start, mut depth, mut quoted) = (Vec::new(), 0, 0i32, false);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' | '(' | '{' if !quoted => depth += 1,
            ']' | ')' | '}' if !quoted => depth -= 1,
            ';' if !quoted && depth <= 0 => {
                out.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&line[start..]);
    out
}

impl Chart {
    fn statement(&mut self, s: &str) {
        if s.is_empty() || s.starts_with("%%") {
            return;
        }
        let word = s.split_whitespace().next().unwrap_or("");
        match word {
            "classDef" | "class" | "style" | "linkStyle" | "click" | "direction" | "accTitle"
            | "accDescr" => return,
            "end" if s == "end" => {
                self.open.pop();
                return;
            }
            "subgraph" => {
                let rest = s["subgraph".len()..].trim();
                let (id, title) = match rest.find(['[', '"']) {
                    Some(i) => {
                        let id = rest[..i].trim();
                        let title = unwrap_text(rest[i..].trim());
                        let id = if id.is_empty() {
                            title.clone()
                        } else {
                            id.to_string()
                        };
                        (id, title)
                    }
                    None => (rest.to_string(), rest.to_string()),
                };
                let id = if id.is_empty() {
                    format!("subgraph-{}", self.subgraphs.len() + 1)
                } else {
                    id
                };
                let parent = self.open.last().map(|(p, _)| p.clone());
                self.subgraphs.push((id.clone(), title.clone(), parent));
                self.open.push((id, title));
                return;
            }
            _ => {}
        }
        let mut p = Parser { s, at: 0 };
        let Some(mut left) = self.group(&mut p) else {
            return;
        };
        // Links and the node groups after them, as long as the statement goes on.
        while let Some(link) = p.link() {
            let Some(right) = self.group(&mut p) else {
                break;
            };
            for from in &left {
                for to in &right {
                    if self.edges.len() < MAX_EDGES {
                        self.edges.push(ChartEdge {
                            from: from.clone(),
                            to: to.clone(),
                            label: link.label.clone(),
                            dash: link.dash,
                            thick: link.thick,
                            arrows: link.arrows,
                        });
                    }
                }
            }
            left = right;
        }
    }

    /// `a & b & c`: their ids, each node declared (with its shape and text) as it's read.
    fn group(&mut self, p: &mut Parser) -> Option<Vec<String>> {
        let mut ids = Vec::new();
        loop {
            p.space();
            let (id, label, shape) = p.node()?;
            self.declare(&id, label, shape);
            ids.push(id);
            p.space();
            if !p.eat("&") {
                break;
            }
        }
        Some(ids)
    }

    fn declare(&mut self, id: &str, label: Option<String>, shape: Option<NodeShape>) {
        if let Some(&i) = self.index.get(id) {
            // A later mention can give the node its text and shape.
            let node = &mut self.nodes[i];
            if label.is_some() {
                node.1 = label;
            }
            if let Some(shape) = shape {
                node.2 = shape;
            }
            return;
        }
        if self.nodes.len() >= MAX_NODES {
            return;
        }
        if let Some((group, _)) = self.open.last() {
            self.member.insert(id.to_string(), group.clone());
        }
        self.index.insert(id.to_string(), self.nodes.len());
        self.nodes
            .push((id.to_string(), label, shape.unwrap_or(NodeShape::Rectangle)));
    }

    fn into_flow(self) -> FlowObject {
        let sizes: Vec<(f32, f32)> = self
            .nodes
            .iter()
            .map(|(id, label, shape)| {
                let text = label.as_deref().unwrap_or(id);
                let (dw, dh) = shape.default_size();
                // Wide enough for the text on a line or two.
                let w = (text.chars().count() as f32 * 7.5 / 1.6 + 40.).clamp(dw, 260.);
                (w, dh)
            })
            .collect();
        let pairs: Vec<(usize, usize)> = self
            .edges
            .iter()
            .filter_map(|e| Some((*self.index.get(&e.from)?, *self.index.get(&e.to)?)))
            .collect();
        let places = layered(self.nodes.len(), &pairs, &sizes, self.direction);
        let mut nodes: Vec<Node> = self
            .nodes
            .iter()
            .zip(places)
            .zip(&sizes)
            .map(|(((id, label, shape), at), (w, h))| {
                let label = label.clone().unwrap_or_else(|| id.clone());
                Node::new(id.clone(), at)
                    .shape(*shape)
                    .label(label)
                    .size(*w, *h)
            })
            .collect();
        frame_subgraphs(&mut nodes, &self.subgraphs, &self.member);
        let (source_side, target_side) = match self.direction {
            Dir::Down => ("bottom", "top"),
            Dir::Up => ("top", "bottom"),
            Dir::Right => ("right", "left"),
            Dir::Left => ("left", "right"),
        };
        let mut seen = HashMap::new();
        let edges = self
            .edges
            .into_iter()
            .filter(|e| self.index.contains_key(&e.from) && self.index.contains_key(&e.to))
            .map(|e| {
                let n = seen.entry((e.from.clone(), e.to.clone())).or_insert(0);
                *n += 1;
                let id = if *n == 1 {
                    format!("{}-{}", e.from, e.to)
                } else {
                    format!("{}-{}-{n}", e.from, e.to)
                };
                let mut edge = Edge::new(id, e.from, e.to)
                    .kind(EdgeKind::SmoothStep)
                    .source_handle(source_side)
                    .target_handle(target_side)
                    .dash(e.dash)
                    .arrows(e.arrows);
                if e.thick {
                    edge.stroke_width = Some(2.5);
                }
                edge.label = e.label.map(Into::into);
                edge
            })
            .collect();
        FlowObject {
            nodes,
            edges,
            viewport: Viewport::default(),
        }
    }
}

/// Puts each subgraph's nodes in a frame round them, titled as the subgraph, inner frames in
/// outer ones.
fn frame_subgraphs(
    nodes: &mut Vec<Node>,
    subgraphs: &[(String, String, Option<String>)],
    member: &HashMap<String, String>,
) {
    const PAD: f32 = 24.;
    const TITLE: f32 = 28.;
    // Innermost first, so an outer frame takes in the inner frames' boxes.
    let depth = |id: &String| {
        let mut d = 0;
        let mut at = subgraphs
            .iter()
            .find(|s| &s.0 == id)
            .and_then(|s| s.2.clone());
        while let Some(p) = at {
            d += 1;
            if d > 64 {
                break;
            }
            at = subgraphs
                .iter()
                .find(|s| s.0 == p)
                .and_then(|s| s.2.clone());
        }
        d
    };
    let mut order: Vec<&(String, String, Option<String>)> = subgraphs.iter().collect();
    order.sort_by_key(|s| std::cmp::Reverse(depth(&s.0)));
    // Each node's box in the flow, frames included as they're made.
    let mut boxes: HashMap<String, Rect> = nodes
        .iter()
        .map(|n| {
            (
                n.id.to_string(),
                Rect::new(
                    n.position.x,
                    n.position.y,
                    n.width.unwrap_or(0.),
                    n.height.unwrap_or(0.),
                ),
            )
        })
        .collect();
    let mut parent_of: HashMap<String, String> = HashMap::new();
    for (id, title, _) in order {
        if boxes.contains_key(id) {
            continue;
        }
        let inside: Vec<String> = member
            .iter()
            .filter(|(_, g)| *g == id)
            .map(|(n, _)| n.clone())
            .chain(
                subgraphs
                    .iter()
                    .filter(|s| s.2.as_ref() == Some(id))
                    .map(|s| s.0.clone()),
            )
            .filter(|n| boxes.contains_key(n) && !parent_of.contains_key(n))
            .collect();
        let Some(area) = inside
            .iter()
            .filter_map(|n| boxes.get(n))
            .copied()
            .reduce(|a, b| a.union(&b))
        else {
            continue;
        };
        let frame = Rect::new(
            area.x - PAD,
            area.y - PAD - TITLE,
            area.w + PAD * 2.,
            area.h + PAD * 2. + TITLE,
        );
        boxes.insert(id.clone(), frame);
        for n in inside {
            parent_of.insert(n, id.clone());
        }
        nodes.push(
            Node::new(id.clone(), xy(frame.x, frame.y))
                .kind("frame")
                .label(title.clone())
                .size(frame.w, frame.h),
        );
    }
    // Positions within each parent frame.
    for n in nodes.iter_mut() {
        if let Some(p) = parent_of.get(n.id.as_ref())
            && let Some(pb) = boxes.get(p)
        {
            n.parent = Some(p.clone().into());
            n.position = n.position - xy(pb.x, pb.y);
        }
    }
    // Frames go first, so they're drawn under what's in them.
    nodes.sort_by_key(|n| n.kind != "frame");
}

/// Layered placement: each node's layer is the longest path to it (loops broken), nodes in a
/// layer ordered to sit near what they join, and layers stacked in the chart's direction.
fn layered(count: usize, edges: &[(usize, usize)], sizes: &[(f32, f32)], dir: Dir) -> Vec<XY> {
    const LAYER_GAP: f32 = 70.;
    const NODE_GAP: f32 = 40.;
    // Edges that close a loop (found by a depth-first walk) don't count toward layers.
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(a, b) in edges {
        if a != b {
            out[a].push(b);
        }
    }
    let mut state = vec![0u8; count];
    let mut forward: Vec<(usize, usize)> = Vec::new();
    for start in 0..count {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state[start] = 1;
        while let Some((v, i)) = stack.pop() {
            if i < out[v].len() {
                stack.push((v, i + 1));
                let w = out[v][i];
                match state[w] {
                    0 => {
                        forward.push((v, w));
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => {} // back to an open node: a loop
                    _ => forward.push((v, w)),
                }
            } else {
                state[v] = 2;
            }
        }
    }
    // Longest path, in topological order.
    let mut incoming = vec![0usize; count];
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); count];
    for &(a, b) in &forward {
        incoming[b] += 1;
        next[a].push(b);
    }
    let mut layer = vec![0usize; count];
    let mut ready: Vec<usize> = (0..count).filter(|v| incoming[*v] == 0).collect();
    ready.reverse();
    while let Some(v) = ready.pop() {
        for &w in &next[v] {
            layer[w] = layer[w].max(layer[v] + 1);
            incoming[w] -= 1;
            if incoming[w] == 0 {
                ready.push(w);
            }
        }
    }
    let layers = layer.iter().copied().max().map_or(0, |m| m + 1);
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for v in 0..count {
        rows[layer[v]].push(v);
    }
    // Two sweeps: each node toward the middle of the ones it joins in the layer before.
    let mut pos = vec![0f32; count];
    let place = |rows: &Vec<Vec<usize>>, pos: &mut Vec<f32>| {
        for row in rows {
            for (i, v) in row.iter().enumerate() {
                pos[*v] = i as f32;
            }
        }
    };
    place(&rows, &mut pos);
    let neighbours = |v: usize, up: bool| -> Vec<usize> {
        forward
            .iter()
            .filter_map(|&(a, b)| match up {
                true if b == v => Some(a),
                false if a == v => Some(b),
                _ => None,
            })
            .collect()
    };
    for sweep in 0..4 {
        let up = sweep % 2 == 0;
        let range: Vec<usize> = if up {
            (1..layers).collect()
        } else {
            (0..layers.saturating_sub(1)).rev().collect()
        };
        for l in range {
            let mut keyed: Vec<(f32, usize)> = rows[l]
                .iter()
                .map(|&v| {
                    let n = neighbours(v, up);
                    let key = if n.is_empty() {
                        pos[v]
                    } else {
                        n.iter().map(|u| pos[*u]).sum::<f32>() / n.len() as f32
                    };
                    (key, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            rows[l] = keyed.into_iter().map(|(_, v)| v).collect();
            for (i, v) in rows[l].iter().enumerate() {
                pos[*v] = i as f32;
            }
        }
    }
    // Coordinates: layers along the direction, each centred across it.
    let along = |v: usize| match dir {
        Dir::Down | Dir::Up => sizes[v].1,
        Dir::Right | Dir::Left => sizes[v].0,
    };
    let across = |v: usize| match dir {
        Dir::Down | Dir::Up => sizes[v].0,
        Dir::Right | Dir::Left => sizes[v].1,
    };
    let mut at = vec![XY::default(); count];
    let mut offset = 0f32;
    for row in &rows {
        let depth = row.iter().map(|&v| along(v)).fold(0., f32::max);
        let width: f32 = row.iter().map(|&v| across(v)).sum::<f32>()
            + NODE_GAP * row.len().saturating_sub(1) as f32;
        let mut x = -width / 2.;
        for &v in row {
            // Centred in its layer's depth.
            let a = offset + (depth - along(v)) / 2.;
            at[v] = match dir {
                Dir::Down => xy(x, a),
                Dir::Up => xy(x, -a - along(v)),
                Dir::Right => xy(a, x),
                Dir::Left => xy(-a - along(v), x),
            };
            x += across(v) + NODE_GAP;
        }
        offset += depth + LAYER_GAP;
    }
    at
}

/// What a link between two node groups says.
struct Link {
    label: Option<String>,
    dash: EdgeDash,
    thick: bool,
    arrows: Arrows,
}

struct Parser<'a> {
    s: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.s[self.at..]
    }

    fn space(&mut self) {
        let skipped = self.rest().len() - self.rest().trim_start().len();
        self.at += skipped;
    }

    fn eat(&mut self, t: &str) -> bool {
        if self.rest().starts_with(t) {
            self.at += t.len();
            true
        } else {
            false
        }
    }

    /// A node: its id, then optionally its shape and text.
    fn node(&mut self) -> Option<(String, Option<String>, Option<NodeShape>)> {
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '/')))
            .map_or(rest.len(), |(i, _)| i);
        // An id doesn't end in what starts a link (`a-->b`), nor is it one.
        let mut id = &rest[..len];
        while id.ends_with('.') {
            id = &id[..id.len() - 1];
        }
        if id.is_empty() {
            return None;
        }
        let id = id.to_string();
        self.at += id.len();
        // Shapes, longest openers first.
        const SHAPES: [(&str, &str, NodeShape); 15] = [
            ("(((", ")))", NodeShape::Ellipse),
            ("((", "))", NodeShape::Ellipse),
            ("([", "])", NodeShape::Rounded),
            ("[[", "]]", NodeShape::Rectangle),
            ("[(", ")]", NodeShape::Cylinder),
            ("[/", "/]", NodeShape::Parallelogram),
            ("[/", "\\]", NodeShape::Parallelogram),
            ("[\\", "\\]", NodeShape::Parallelogram),
            ("[\\", "/]", NodeShape::Parallelogram),
            ("{{", "}}", NodeShape::Hexagon),
            ("[", "]", NodeShape::Rectangle),
            ("(", ")", NodeShape::Rounded),
            ("{", "}", NodeShape::Diamond),
            (">", "]", NodeShape::Parallelogram),
            ("@{", "}", NodeShape::Rectangle),
        ];
        for (open, close, shape) in SHAPES {
            if !self.rest().starts_with(open) {
                continue;
            }
            let body_start = self.at + open.len();
            let Some(end) = self.s[body_start..].find(close) else {
                continue;
            };
            let body = &self.s[body_start..body_start + end];
            self.at = body_start + end + close.len();
            return Some((id, Some(unwrap_text(body)), Some(shape)));
        }
        Some((id, None, None))
    }

    /// A link: `-->`, `---`, `-.->`, `==>`, `<-->`, `--o`, `-- text -->`, and a `|text|` after.
    fn link(&mut self) -> Option<Link> {
        self.space();
        let rest = self.rest();
        let start = rest.starts_with('<');
        let body_from = usize::from(start);
        let run = rest[body_from..]
            .char_indices()
            .find(|(_, c)| !matches!(c, '-' | '=' | '.'))
            .map_or(rest.len() - body_from, |(i, _)| i);
        if run < 2 {
            return None;
        }
        let mut token = rest[..body_from + run].to_string();
        let mut after = &rest[body_from + run..];
        // An arrowhead: `>`, or a circle or cross before a space.
        let mut head = false;
        if let Some(r) = after.strip_prefix('>') {
            head = true;
            token.push('>');
            after = r;
        } else if (after.starts_with('o') || after.starts_with('x'))
            && after[1..].starts_with(char::is_whitespace)
        {
            head = true;
            after = &after[1..];
        }
        let mut label = None;
        // `-- text -->`: an opener with text and then the rest of the link.
        if !head && run <= 3 && after.starts_with(char::is_whitespace) {
            let closers = ["-->", "---", "==>", "===", ".->", ".-", "--o", "--x"];
            let found = closers
                .iter()
                .filter_map(|c| after.find(c).map(|i| (i, *c)))
                .min_by_key(|(i, _)| *i);
            if let Some((i, closer)) = found {
                let text = after[..i].trim();
                if !text.is_empty() {
                    label = Some(unwrap_text(text));
                    token.push_str(closer);
                    head = closer.ends_with('>') || closer.ends_with('o') || closer.ends_with('x');
                    // Past the closer's own dashes.
                    let tail = &after[i + closer.len()..];
                    let extra = tail
                        .char_indices()
                        .find(|(_, c)| !matches!(c, '-' | '=' | '.' | '>'))
                        .map_or(tail.len(), |(j, _)| j);
                    if tail[..extra].contains('>') {
                        head = true;
                    }
                    after = &tail[extra..];
                }
            }
        }
        let consumed = rest.len() - after.len();
        self.at += consumed;
        self.space();
        // `-->|text|`
        if self.rest().starts_with('|')
            && let Some(end) = self.rest()[1..].find('|')
        {
            label = Some(unwrap_text(&self.rest()[1..1 + end]));
            self.at += end + 2;
        }
        Some(Link {
            label: label.filter(|l| !l.is_empty()),
            dash: if token.contains('.') {
                EdgeDash::Dotted
            } else {
                EdgeDash::Solid
            },
            thick: token.contains('='),
            arrows: match (start, head) {
                (true, true) => Arrows::Both,
                (true, false) => Arrows::Reverse,
                (false, true) => Arrows::Forward,
                (false, false) => Arrows::None,
            },
        })
    }
}

/// A node's or a link's text: quotes and Markdown back-quotes taken off, `<br>` as a new line,
/// and the few entities Mermaid writes made plain.
fn unwrap_text(raw: &str) -> String {
    let mut t = raw.trim();
    for (open, close) in [("\"`", "`\""), ("\"", "\""), ("[", "]"), ("`", "`")] {
        if t.len() >= open.len() + close.len() && t.starts_with(open) && t.ends_with(close) {
            t = &t[open.len()..t.len() - close.len()];
        }
    }
    t.replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("#quot;", "\"")
        .chars()
        .take(MAX_TEXT)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node<'a>(f: &'a FlowObject, id: &str) -> &'a Node {
        f.nodes.iter().find(|n| n.id == id).unwrap()
    }

    #[test]
    fn a_flowchart_keeps_its_shapes_text_and_arrows() {
        let f = from_mermaid(
            "flowchart TD\n  A[Start] --> B{Is it?}\n  B -->|Yes| C(Round)\n  B -- No --> D[(Database)]\n  C -.-> E((Circle)) ==> F{{Hex}}\n  D <--> F\n  G[/Slanted/] --- A\n  click A \"https://evil.example\"\n",
        )
        .unwrap();
        assert_eq!(node(&f, "A").label_text(), "Start");
        assert_eq!(node(&f, "B").shape, Some(NodeShape::Diamond));
        assert_eq!(node(&f, "C").shape, Some(NodeShape::Rounded));
        assert_eq!(node(&f, "D").shape, Some(NodeShape::Cylinder));
        assert_eq!(node(&f, "E").shape, Some(NodeShape::Ellipse));
        assert_eq!(node(&f, "F").shape, Some(NodeShape::Hexagon));
        assert_eq!(node(&f, "G").shape, Some(NodeShape::Parallelogram));
        let edge = |a: &str, b: &str| {
            f.edges
                .iter()
                .find(|e| e.source == a && e.target == b)
                .unwrap()
        };
        assert_eq!(edge("B", "C").label.as_deref(), Some("Yes"));
        assert_eq!(edge("B", "D").label.as_deref(), Some("No"));
        assert_eq!(edge("C", "E").dash, EdgeDash::Dotted);
        assert_eq!(edge("E", "F").stroke_width, Some(2.5));
        let both = edge("D", "F");
        assert!(both.marker_start.is_some() && both.marker_end.is_some());
        let open = edge("G", "A");
        assert!(open.marker_start.is_none() && open.marker_end.is_none());
        // `click` made nothing: no node links anywhere.
        assert!(f.nodes.iter().all(|n| n.link.is_none() && n.card.is_none()));
    }

    #[test]
    fn layers_follow_the_direction() {
        let down = from_mermaid("graph TD\nA-->B\nB-->C\nA-->C").unwrap();
        let (a, b, c) = (node(&down, "A"), node(&down, "B"), node(&down, "C"));
        assert!(a.position.y < b.position.y && b.position.y < c.position.y);
        let right = from_mermaid("graph LR\nA-->B").unwrap();
        assert!(node(&right, "A").position.x < node(&right, "B").position.x);
        // A loop doesn't stop it.
        let looped = from_mermaid("graph TD\nA-->B\nB-->A").unwrap();
        assert_eq!(looped.nodes.len(), 2);
    }

    #[test]
    fn chains_and_ampersands_make_every_edge() {
        let f = from_mermaid("graph TD\nA & B --> C --> D & E").unwrap();
        let pairs: Vec<(&str, &str)> = f
            .edges
            .iter()
            .map(|e| (e.source.as_ref(), e.target.as_ref()))
            .collect();
        for p in [("A", "C"), ("B", "C"), ("C", "D"), ("C", "E")] {
            assert!(pairs.contains(&p), "{p:?} in {pairs:?}");
        }
    }

    #[test]
    fn subgraphs_become_frames_round_their_nodes() {
        let f = from_mermaid(
            "flowchart LR\n  subgraph pay [Payments]\n    A[Charge] --> B[Settle]\n  end\n  B --> C[Ledger]\n",
        )
        .unwrap();
        let frame = node(&f, "pay");
        assert_eq!(
            (frame.kind.as_ref(), frame.label_text().as_ref()),
            ("frame", "Payments")
        );
        assert_eq!(node(&f, "A").parent.as_deref(), Some("pay"));
        assert_eq!(node(&f, "C").parent, None);
        // The frame is listed first, so it's drawn under its nodes.
        assert_eq!(f.nodes[0].id, "pay");
    }

    #[test]
    fn hostile_mermaid_never_panics() {
        for src in [
            "",
            "pie title x",
            "graph TD\n[[[[",
            "graph TD\nA-->",
            "graph TD\n-->-->-->",
            "graph TD\nA[\"unclosed --> B",
            "graph TD\nA|x|B",
            "graph\nsubgraph\nend\nend\nend",
            &format!("graph TD\n{}", "A-->".repeat(10_000)),
            "graph TD\nA--o B --x C <-- D",
            "graph TD\nA -- text without end B",
            "graph TD\n日本 --> العربية --> 🎓",
        ] {
            let _ = from_mermaid(src);
        }
        assert!(from_mermaid("sequenceDiagram\nA->>B: hi").is_err());
    }

    #[test]
    fn a_subcanvas_whiteboard_comes_across() {
        let json = r#"{
            "whiteboard_id": "w", "title": "Services",
            "nodes": [
                { "id": "g", "kind": "group", "title": "Payments", "x": 0, "y": 0, "width": 400, "height": 300 },
                { "id": "a", "kind": "plain", "title": "Charge", "x": 20, "y": 40, "width": 160, "height": 64,
                  "group_id": "g", "color": "blue", "shape": "diamond", "icon": "credit-card", "emoji": "💳",
                  "doc_id": "doc-1", "doc_type": "text", "open_mode": "panel" },
                { "id": "b", "kind": "plain", "title": "Ledger", "x": 600, "y": 40, "width": null, "height": null },
                { "id": "t", "kind": "text", "title": "Notes", "x": 0, "y": 400, "width": 240, "height": null },
                { "id": "m", "kind": "media", "title": "Diagram", "x": 300, "y": 400, "width": 320, "height": 180,
                  "media": { "type": "image", "width": 1600, "height": 900, "alt": "The flow", "url": "https://subcanvas.example/api/media/x.png" } },
                { "id": "orphan", "kind": "plain", "title": "Loose", "x": 0, "y": 0, "group_id": "missing" }
            ],
            "edges": [
                { "id": "e", "source": "a", "target": "b", "label": "posts", "direction": "both",
                  "shape": "step", "stroke": "dotted", "color": "green" },
                { "id": "gone", "source": "a", "target": "nowhere", "direction": "forward" }
            ]
        }"#;
        let f = from_subcanvas(json).unwrap();
        let a = node(&f, "a");
        assert_eq!(a.kind, "shape");
        assert_eq!(a.shape, Some(NodeShape::Diamond));
        assert_eq!(a.parent.as_deref(), Some("g"));
        assert_eq!(a.color.as_deref(), Some("blue"));
        assert_eq!(
            (a.icon.as_deref(), a.emoji.as_deref()),
            (Some("credit-card"), Some("💳"))
        );
        assert_eq!(a.link.as_deref(), Some("doc-1"));
        assert_eq!(node(&f, "g").kind, "frame");
        assert_eq!(node(&f, "t").kind, "text");
        // A shape with no size takes its shape's.
        assert_eq!(node(&f, "b").width, Some(160.));
        let m = node(&f, "m");
        assert_eq!(m.kind, "image");
        let media = m.media.as_ref().unwrap();
        assert_eq!((media.width, media.alt.as_ref()), (Some(1600.), "The flow"));
        assert_eq!(node(&f, "orphan").parent, None);
        assert_eq!(f.edges.len(), 1);
        let e = &f.edges[0];
        assert_eq!(e.kind, EdgeKind::SmoothStep);
        assert_eq!(e.dash, EdgeDash::Dotted);
        assert_eq!(e.pencil.as_deref(), Some("green"));
        assert!(e.marker_start.is_some() && e.marker_end.is_some());
        // a is in the group at (20, 40); b to its right: they meet on facing sides.
        assert_eq!(
            (e.source_handle.as_deref(), e.target_handle.as_deref()),
            (Some("right"), Some("left"))
        );
    }

    #[test]
    fn hostile_subcanvas_json_never_panics() {
        for src in [
            "",
            "[]",
            "{}",
            r#"{"nodes": 7}"#,
            r#"{"nodes": [1, null, {"id": 3}, {"id": "a", "x": "far", "width": -1, "group_id": "a"}]}"#,
            r#"{"nodes": [{"id": "a", "kind": "media", "media": "x"}], "edges": [{"id": "e", "source": "a", "target": "a"}]}"#,
        ] {
            let _ = from_subcanvas(src);
        }
    }
}
