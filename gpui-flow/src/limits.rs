//! What a stored flow may hold, so a server and every client turn away the same documents and
//! edits. `tests/fixtures/limits` in the repository has the cases both test against.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ink;
use crate::json::{Document, MAX_ID, Op, valid_id};
use crate::types::{Edge, Node};

/// The limits, the defaults being what Docs stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Limits {
    /// Nodes in a document, frames included.
    pub nodes: usize,
    pub edges: usize,
    /// Operations in one batch of edits.
    pub ops: usize,
    /// An id, and the ids an edge names (`source`, `target`, its handles).
    pub id_chars: usize,
    /// A text: a node's `data.label`, an edge's `label`, a picture's `alt`, a card's `title`,
    /// `description` or `site`.
    pub text_chars: usize,
    /// An address or a reference: a node's `link`, a picture's `src`, a card's `url` or
    /// `image`.
    pub url_chars: usize,
    /// How many frames (or groups) deep a node may be: a node in no frame is 0 deep.
    pub depth: usize,
    /// One node as stored, in bytes of JSON (room for a long ink stroke).
    pub node_bytes: usize,
    /// One edge as stored, in bytes of JSON.
    pub edge_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 10_000,
            edges: 20_000,
            ops: 30_000,
            id_chars: MAX_ID,
            text_chars: 10_000,
            url_chars: 2_048,
            depth: 16,
            node_bytes: 1 << 20,
            edge_bytes: 1 << 16,
        }
    }
}

/// One thing over a limit, or wrong, and where: a path in the JSON's own names
/// (`nodes[3].data.label`), and what's wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub kind: ProblemKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    TooMany {
        limit: usize,
        found: usize,
    },
    /// In Unicode characters.
    TooLong {
        limit: usize,
        found: usize,
    },
    TooDeep {
        limit: usize,
        found: usize,
    },
    /// In bytes of compact JSON. Said without the size found, which differs a little from
    /// one serializer to another.
    TooLarge {
        limit: usize,
        found: usize,
    },
    /// Not 1 to [`MAX_ID`] of `A-Z a-z 0-9 - _ . : ~`.
    BadId,
    /// Two nodes, or two edges, share this id.
    Duplicate(String),
    /// An `ink` node whose `data.ink` doesn't read as a stroke (see [`crate::ink`]).
    NotInk,
    /// A stroke's width not more than 0 and at most 200, or a value of a point not a number
    /// within ±20,000.
    OutOfRange,
}

impl fmt::Display for ProblemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany { limit, found } => write!(f, "{found} is more than {limit}"),
            Self::TooLong { limit, found } => {
                write!(f, "{found} characters is more than {limit}")
            }
            Self::TooDeep { limit, found } => write!(f, "{found} levels is more than {limit}"),
            // Without the size found: serializers differ by a few bytes (`1` or `1.0`).
            Self::TooLarge { limit, .. } => write!(f, "more than {limit} bytes"),
            Self::BadId => f.write_str("not a valid id"),
            Self::Duplicate(id) => write!(f, "{id} is used twice"),
            Self::NotInk => f.write_str("not an ink stroke"),
            Self::OutOfRange => f.write_str("out of range"),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.kind)
    }
}

impl Limits {
    /// Every problem a document has.
    pub fn check(&self, doc: &Document) -> Vec<Problem> {
        let mut out = Vec::new();
        self.count("nodes", doc.nodes.len(), self.nodes, &mut out);
        self.count("edges", doc.edges.len(), self.edges, &mut out);
        let mut seen = HashSet::new();
        for (i, node) in doc.nodes.iter().enumerate() {
            let path = format!("nodes[{i}]");
            self.node(&path, node, &mut out);
            if valid_id(&node.id) && !seen.insert(node.id.as_ref()) {
                out.push(problem(
                    format!("{path}.id"),
                    ProblemKind::Duplicate(node.id.to_string()),
                ));
            }
        }
        // How deep each node is, its frames counted up to a loop.
        let parents: HashMap<&str, &str> = doc
            .nodes
            .iter()
            .filter_map(|n| Some((n.id.as_ref(), n.parent.as_deref()?)))
            .collect();
        let ids: HashSet<&str> = doc.nodes.iter().map(|n| n.id.as_ref()).collect();
        for (i, node) in doc.nodes.iter().enumerate() {
            let mut depth = 0;
            let mut seen = HashSet::from([node.id.as_ref()]);
            let mut at = node.id.as_ref();
            while let Some(&parent) = parents.get(at) {
                // A parent that isn't there, or one met again round a loop, ends the count.
                if !ids.contains(parent) || !seen.insert(parent) {
                    break;
                }
                depth += 1;
                at = parent;
            }
            if depth > self.depth {
                out.push(problem(
                    format!("nodes[{i}].parentId"),
                    ProblemKind::TooDeep {
                        limit: self.depth,
                        found: depth,
                    },
                ));
            }
        }
        let mut seen = HashSet::new();
        for (i, edge) in doc.edges.iter().enumerate() {
            let path = format!("edges[{i}]");
            self.edge(&path, edge, &mut out);
            if valid_id(&edge.id) && !seen.insert(edge.id.as_ref()) {
                out.push(problem(
                    format!("{path}.id"),
                    ProblemKind::Duplicate(edge.id.to_string()),
                ));
            }
        }
        out
    }

    /// The problems a batch of edits has on its own: too many, and adds and patches that
    /// break a limit by themselves. Depth, duplicates and counts in the document depend on
    /// what it holds: check the document the batch makes.
    pub fn check_batch(&self, ops: &[Op]) -> Vec<Problem> {
        let mut out = Vec::new();
        self.count("ops", ops.len(), self.ops, &mut out);
        for (i, op) in ops.iter().enumerate() {
            match op {
                Op::AddNode { node } => self.node(&format!("ops[{i}].node"), node, &mut out),
                Op::AddEdge { edge } => self.edge(&format!("ops[{i}].edge"), edge, &mut out),
                Op::PatchNode { id, patch } | Op::PatchEdge { id, patch } => {
                    self.id(&format!("ops[{i}].id"), id, &mut out);
                    let limit = match op {
                        Op::PatchNode { .. } => self.node_bytes,
                        _ => self.edge_bytes,
                    };
                    let bytes = serde_json::to_string(patch).map_or(0, |s| s.len());
                    if bytes > limit {
                        out.push(problem(
                            format!("ops[{i}].patch"),
                            ProblemKind::TooLarge {
                                limit,
                                found: bytes,
                            },
                        ));
                    }
                }
                Op::RemoveNode { id } | Op::RemoveEdge { id } => {
                    self.id(&format!("ops[{i}].id"), id, &mut out)
                }
            }
        }
        out
    }

    fn count(&self, path: &str, found: usize, limit: usize, out: &mut Vec<Problem>) {
        if found > limit {
            out.push(problem(path.into(), ProblemKind::TooMany { limit, found }));
        }
    }

    fn id(&self, path: &str, id: &str, out: &mut Vec<Problem>) {
        let found = id.chars().count();
        if found > self.id_chars {
            out.push(problem(
                path.into(),
                ProblemKind::TooLong {
                    limit: self.id_chars,
                    found,
                },
            ));
        } else if !valid_id(id) {
            out.push(problem(path.into(), ProblemKind::BadId));
        }
    }

    fn chars(&self, path: String, text: Option<&str>, limit: usize, out: &mut Vec<Problem>) {
        let found = text.map_or(0, |t| t.chars().count());
        if found > limit {
            out.push(problem(path, ProblemKind::TooLong { limit, found }));
        }
    }

    fn node(&self, path: &str, node: &Node, out: &mut Vec<Problem>) {
        self.id(&format!("{path}.id"), &node.id, out);
        let (text, url) = (self.text_chars, self.url_chars);
        let label = node.data.get("label").and_then(Value::as_str);
        self.chars(format!("{path}.data.label"), label, text, out);
        self.chars(format!("{path}.link"), node.link.as_deref(), url, out);
        if let Some(p) = &node.parent {
            let found = p.chars().count();
            if found > self.id_chars {
                out.push(problem(
                    format!("{path}.parentId"),
                    ProblemKind::TooLong {
                        limit: self.id_chars,
                        found,
                    },
                ));
            }
        }
        if let Some(m) = &node.media {
            self.chars(format!("{path}.media.src"), Some(&m.src), url, out);
            self.chars(format!("{path}.media.alt"), Some(&m.alt), text, out);
        }
        if let Some(c) = &node.card {
            self.chars(format!("{path}.card.url"), Some(&c.url), url, out);
            self.chars(format!("{path}.card.image"), c.image.as_deref(), url, out);
            for (field, value) in [
                ("title", &c.title),
                ("description", &c.description),
                ("site", &c.site),
            ] {
                self.chars(format!("{path}.card.{field}"), value.as_deref(), text, out);
            }
        }
        if node.kind == "ink" {
            self.ink(path, node, out);
        }
        let bytes = serde_json::to_string(node).map_or(0, |s| s.len());
        if bytes > self.node_bytes {
            out.push(problem(
                path.into(),
                ProblemKind::TooLarge {
                    limit: self.node_bytes,
                    found: bytes,
                },
            ));
        }
    }

    /// An ink node's stroke: one that reads, of at most [`ink::MAX_POINTS`] points, each value
    /// a number within ±[`ink::MAX_COORDINATE`], drawn at a width above 0 and at most
    /// [`ink::MAX_WIDTH`].
    fn ink(&self, path: &str, node: &Node, out: &mut Vec<Problem>) {
        let Some(stroke) = ink::InkStroke::of(node) else {
            out.push(problem(format!("{path}.data.ink"), ProblemKind::NotInk));
            return;
        };
        if stroke.points.len() > ink::MAX_POINTS {
            out.push(problem(
                format!("{path}.data.ink.points"),
                ProblemKind::TooMany {
                    limit: ink::MAX_POINTS,
                    found: stroke.points.len(),
                },
            ));
        }
        let fine = |v: f32| v.is_finite() && v.abs() <= ink::MAX_COORDINATE;
        if !stroke.points.iter().all(|p| {
            [p.x, p.y, p.t, p.force, p.azimuth, p.altitude, p.size]
                .into_iter()
                .all(fine)
        }) {
            out.push(problem(
                format!("{path}.data.ink.points"),
                ProblemKind::OutOfRange,
            ));
        }
        if !(stroke.width > 0. && stroke.width <= ink::MAX_WIDTH) {
            out.push(problem(
                format!("{path}.data.ink.width"),
                ProblemKind::OutOfRange,
            ));
        }
        if stroke
            .transform
            .is_some_and(|t| !t.iter().all(|v| v.is_finite()))
        {
            out.push(problem(
                format!("{path}.data.ink.transform"),
                ProblemKind::OutOfRange,
            ));
        }
    }

    fn edge(&self, path: &str, edge: &Edge, out: &mut Vec<Problem>) {
        self.id(&format!("{path}.id"), &edge.id, out);
        for (field, value) in [
            ("source", Some(&edge.source)),
            ("target", Some(&edge.target)),
            ("sourceHandle", edge.source_handle.as_ref()),
            ("targetHandle", edge.target_handle.as_ref()),
        ] {
            self.chars(
                format!("{path}.{field}"),
                value.map(|v| v.as_ref()),
                self.id_chars,
                out,
            );
        }
        self.chars(
            format!("{path}.label"),
            edge.label.as_deref(),
            self.text_chars,
            out,
        );
        let bytes = serde_json::to_string(edge).map_or(0, |s| s.len());
        if bytes > self.edge_bytes {
            out.push(problem(
                path.into(),
                ProblemKind::TooLarge {
                    limit: self.edge_bytes,
                    found: bytes,
                },
            ));
        }
    }
}

fn problem(path: String, kind: ProblemKind) -> Problem {
    Problem { path, kind }
}
