//! A flow as a stored document: nodes, edges and frames, each with an id that stays the same
//! while the flow is edited, for an app or a server to store, merge and index.
//!
//! ```json
//! {
//!   "format": "gpui-flow",
//!   "version": 1,
//!   "layout": { "type": "tree", "direction": "right" },
//!   "nodes": [
//!     { "id": "k3W9cQe2LfAp", "position": { "x": 0, "y": 0 }, "type": "topic",
//!       "data": { "label": "Launch" }, "link": "4f2a" },
//!     { "id": "Zr81mXbT0qvN", "position": { "x": 0, "y": 0 }, "type": "topic",
//!       "data": { "label": "Press" } }
//!   ],
//!   "edges": [
//!     { "id": "c7HhQ2wLx5Ke", "source": "k3W9cQe2LfAp", "target": "Zr81mXbT0qvN" }
//!   ]
//! }
//! ```
//!
//! Nodes and edges are written as React Flow writes them, with this crate's fields beside
//! React Flow's. A frame is a node of type `frame`, and what's in it has its id as `parentId`.
//! A link to another document is a node's `link`. Fields this crate doesn't know are kept, at
//! every level, so a document from a newer writer saves back without loss. See `docs/json.md`
//! in the repository for every field.
//!
//! [`Flow::document`](crate::Flow::document) and [`Flow::set_document`](crate::Flow::set_document)
//! take a flow's document and give it one; what's selected is the person's own, and is never
//! stored.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::tree::TreeDirection;
use crate::types::{Edge, Node, NodeId, Viewport};

/// The value of `format`.
pub const FORMAT: &str = "gpui-flow";
/// The version this crate writes and reads.
pub const VERSION: u32 = 1;
/// The longest id read; a longer one is replaced.
pub const MAX_ID: usize = 128;

/// A whole flow, as stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Always [`FORMAT`].
    pub format: String,
    /// Always [`VERSION`] when written by this crate.
    pub version: u32,
    /// How the nodes are laid out: where they're put (a whiteboard) when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<DocLayout>,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Where the flow was in view when it was saved. A host may keep its own per person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
    /// Fields of a newer writer's, kept for saving.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// How a document's nodes are laid out.
#[derive(Clone, Debug, PartialEq)]
pub enum DocLayout {
    /// A mind map: a tree read from the edges, growing this way.
    Tree { direction: TreeDirection },
    /// A layout this crate doesn't know, kept as it was. The flow lays it out freely.
    Other(Value),
}

impl Serialize for DocLayout {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            DocLayout::Tree { direction } => {
                serde_json::json!({ "type": "tree", "direction": direction }).serialize(s)
            }
            DocLayout::Other(v) => v.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for DocLayout {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        if value.get("type").and_then(Value::as_str) == Some("tree") {
            let direction = value
                .get("direction")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            return Ok(DocLayout::Tree { direction });
        }
        Ok(DocLayout::Other(value))
    }
}

/// Why JSON couldn't be read.
#[derive(Debug)]
pub enum Error {
    Json(serde_json::Error),
    /// Not this format, or a newer version.
    Format {
        format: String,
        version: u32,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "not a flow document: {e}"),
            Self::Format { format, version } => {
                write!(f, "can't read {format:?} version {version}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl Default for Document {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new())
    }
}

impl Document {
    /// A document of these nodes and edges, laid out freely.
    pub fn new(nodes: Vec<Node>, edges: Vec<Edge>) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            layout: None,
            nodes,
            edges,
            viewport: None,
            extra: Map::new(),
        }
    }

    /// Reads a document from JSON. Ids that are missing, invalid or repeated are replaced
    /// (see [`Document::fix_ids`]), and nothing is selected.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let mut doc: Self = serde_json::from_str(json).map_err(Error::Json)?;
        if doc.format != FORMAT || doc.version > VERSION {
            return Err(Error::Format {
                format: doc.format,
                version: doc.version,
            });
        }
        doc.fix_ids();
        doc.deselect();
        Ok(doc)
    }

    /// The document as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.stored()).unwrap_or_default()
    }

    /// The document as JSON, indented for people.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.stored()).unwrap_or_default()
    }

    /// As written: with nothing selected, and as this version.
    fn stored(&self) -> Self {
        let mut doc = self.clone();
        doc.format = FORMAT.into();
        doc.version = VERSION;
        doc.deselect();
        doc
    }

    fn deselect(&mut self) {
        self.nodes.iter_mut().for_each(|n| n.selected = false);
        self.edges.iter_mut().for_each(|e| e.selected = false);
    }

    /// Makes every id valid and unique: a node's or an edge's that is invalid gets a new one
    /// (and what pointed at an invalid node id points at its new one), and one repeated gets a
    /// new one for every use after the first (and what pointed at it points at the first).
    pub fn fix_ids(&mut self) {
        let mut renamed: HashMap<NodeId, NodeId> = HashMap::new();
        let mut seen: HashSet<NodeId> = HashSet::new();
        for node in &mut self.nodes {
            if !valid_id(&node.id) {
                let id: NodeId = new_id().into();
                renamed.insert(node.id.clone(), id.clone());
                node.id = id;
            } else if seen.contains(&node.id) {
                node.id = new_id().into();
            }
            seen.insert(node.id.clone());
        }
        if !renamed.is_empty() {
            let rename = |id: &mut NodeId| {
                if let Some(new) = renamed.get(id) {
                    *id = new.clone();
                }
            };
            for node in &mut self.nodes {
                if let Some(parent) = &mut node.parent {
                    rename(parent);
                }
            }
            for edge in &mut self.edges {
                rename(&mut edge.source);
                rename(&mut edge.target);
            }
        }
        let mut seen: HashSet<NodeId> = HashSet::new();
        for edge in &mut self.edges {
            if !valid_id(&edge.id) || seen.contains(&edge.id) {
                edge.id = new_id().into();
            }
            seen.insert(edge.id.clone());
        }
    }

    /// Every node's `link`, once each, in the order of the nodes: the documents this one
    /// links to.
    pub fn links(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.nodes
            .iter()
            .filter_map(|n| n.link.as_deref())
            .filter(|l| !l.is_empty() && seen.insert(*l))
            .map(str::to_string)
            .collect()
    }
}

/// Whether an id may be stored: 1 to [`MAX_ID`] bytes of `A-Z a-z 0-9 - _ . : ~`.
pub fn valid_id(id: &str) -> bool {
    (1..=MAX_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'~'))
}

/// A new id: 12 characters from 62, about 71 bits, so two people (or a person and an agent)
/// adding at once don't make the same one.
pub fn new_id() -> String {
    // In a browser std's clock panics; web-time's reads the browser's.
    #[cfg(not(target_family = "wasm"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(target_family = "wasm")]
    use web_time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut hasher = std::hash::RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    let mut a = hasher.finish();
    let mut b = std::hash::RandomState::new().build_hasher().finish() ^ a.rotate_left(17);
    (0..12)
        .map(|i| {
            let n = if i < 6 { &mut a } else { &mut b };
            let c = ALPHABET[(*n % 62) as usize] as char;
            *n /= 62;
            c
        })
        .collect()
}

/// One change to a stored flow, by id. The flow emits them in batches
/// ([`FlowEvent::Edits`](crate::FlowEvent::Edits)), one batch for each settled edit (a drag
/// of many nodes is one batch), and applies batches from elsewhere with
/// [`Flow::apply`](crate::Flow::apply).
///
/// Written as JSON with an `op` tag:
/// `{"op": "patchNode", "id": "k3W9cQe2LfAp", "patch": {"position": {"x": 40}}}`.
/// A frame is a node, so frames change by the node operations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    /// A new node, as stored. One whose id is taken replaces it.
    AddNode {
        node: Box<Node>,
    },
    /// Some of a node's fields changed: a JSON merge patch (RFC 7386) of its stored form, with
    /// only what changed. An object is merged field by field, anything else replaces, and
    /// `null` takes a field away.
    PatchNode {
        id: NodeId,
        patch: Map<String, Value>,
    },
    /// A node went. Its children and edges go by their own operations in the same batch.
    RemoveNode {
        id: NodeId,
    },
    AddEdge {
        edge: Box<Edge>,
    },
    PatchEdge {
        id: crate::types::EdgeId,
        patch: Map<String, Value>,
    },
    RemoveEdge {
        id: crate::types::EdgeId,
    },
}

impl Op {
    /// The id of the node or edge it changes.
    pub fn id(&self) -> &str {
        match self {
            Op::AddNode { node } => &node.id,
            Op::AddEdge { edge } => &edge.id,
            Op::PatchNode { id, .. }
            | Op::RemoveNode { id }
            | Op::PatchEdge { id, .. }
            | Op::RemoveEdge { id } => id,
        }
    }
}

/// Applies a JSON merge patch (RFC 7386) to `target`.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(fields) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let Value::Object(into) = target else {
        return;
    };
    for (key, value) in fields {
        if value.is_null() {
            into.remove(key);
        } else {
            merge_patch(into.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

/// The merge patch that makes `old` into `new`, or none when they're the same. Objects are
/// compared field by field; anything else is replaced whole. (A `null` inside `new` can't be
/// said by a merge patch, and comes out as the field taken away.)
pub fn merge_diff(old: &Value, new: &Value) -> Option<Value> {
    if old == new {
        return None;
    }
    let (Value::Object(a), Value::Object(b)) = (old, new) else {
        return Some(new.clone());
    };
    let mut patch = Map::new();
    for (key, value) in b {
        match a.get(key) {
            Some(was) => {
                if let Some(p) = merge_diff(was, value) {
                    patch.insert(key.clone(), p);
                }
            }
            None => {
                patch.insert(key.clone(), value.clone());
            }
        }
    }
    for key in a.keys() {
        if !b.contains_key(key) {
            patch.insert(key.clone(), Value::Null);
        }
    }
    Some(Value::Object(patch))
}

/// The fields of `new` that differ from `old`, as a patch of their stored forms. Selection
/// isn't stored, so it's never in one.
pub(crate) fn diff_stored<T: Serialize>(old: &T, new: &T) -> Map<String, Value> {
    let (Ok(a), Ok(b)) = (serde_json::to_value(old), serde_json::to_value(new)) else {
        return Map::new();
    };
    let mut patch = match merge_diff(&a, &b) {
        Some(Value::Object(p)) => p,
        _ => Map::new(),
    };
    patch.remove("selected");
    patch
}

/// `item` with `patch` merged into its stored form; none when the result doesn't read back.
/// The id and the selection stay as they were.
pub(crate) fn patched<T>(item: &T, patch: &Map<String, Value>) -> Option<T>
where
    T: Serialize + serde::de::DeserializeOwned,
{
    let mut value = serde_json::to_value(item).ok()?;
    let keep_id = value.get("id").cloned();
    let keep_selected = value.get("selected").cloned();
    merge_patch(&mut value, &Value::Object(patch.clone()));
    let fields = value.as_object_mut()?;
    match keep_id {
        Some(id) => fields.insert("id".into(), id),
        None => fields.remove("id"),
    };
    match keep_selected {
        Some(s) => fields.insert("selected".into(), s),
        None => fields.remove("selected"),
    };
    serde_json::from_value(value).ok()
}

impl Document {
    /// Applies a batch of operations, as a server would to the document it stores. A patch or
    /// removal of an id that isn't there is skipped, an add of one that is replaces it, and an
    /// add with an invalid id is skipped.
    pub fn apply(&mut self, ops: &[Op]) {
        for op in ops {
            match op {
                Op::AddNode { node } => {
                    if !valid_id(&node.id) {
                        continue;
                    }
                    let mut node = (**node).clone();
                    node.selected = false;
                    match self.nodes.iter_mut().find(|n| n.id == node.id) {
                        Some(n) => *n = node,
                        None => self.nodes.push(node),
                    }
                }
                Op::PatchNode { id, patch } => {
                    if let Some(n) = self.nodes.iter_mut().find(|n| &n.id == id)
                        && let Some(new) = patched(n, patch)
                    {
                        *n = new;
                    }
                }
                Op::RemoveNode { id } => self.nodes.retain(|n| &n.id != id),
                Op::AddEdge { edge } => {
                    if !valid_id(&edge.id) {
                        continue;
                    }
                    let mut edge = (**edge).clone();
                    edge.selected = false;
                    match self.edges.iter_mut().find(|e| e.id == edge.id) {
                        Some(e) => *e = edge,
                        None => self.edges.push(edge),
                    }
                }
                Op::PatchEdge { id, patch } => {
                    if let Some(e) = self.edges.iter_mut().find(|e| &e.id == id)
                        && let Some(new) = patched(e, patch)
                    {
                        *e = new;
                    }
                }
                Op::RemoveEdge { id } => self.edges.retain(|e| &e.id != id),
            }
        }
    }
}
