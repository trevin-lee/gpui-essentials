//! Copy, cut, paste and duplicate: the selection as a document of its own (gpui-flow's stored
//! JSON, on the clipboard as text), put back with new ids, a step along, as one edit.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Styled as _, WeakEntity, div, px,
};
use gpui_core::{Button, ButtonVariants as _, ControlSize, Sizable as _};

use super::{Flow, sanitize_edge, sanitize_node};
use crate::json::{self, DocLayout, Document};
use crate::theme::FlowColors;
use crate::types::*;

/// How far each paste of the same thing, and each duplicate, lands from the last (flow units).
pub(super) const PASTE_STEP: f32 = 20.;

/// Where pasted nodes go in a tree: under one node, or each under its own.
enum Attach {
    /// Each top node under the node selected when it was pasted: a subtree pasted as its child.
    Under(NodeId),
    /// Each top node under its original's parent, after it: a duplicate as its sibling.
    BesideOriginals,
    /// Nowhere: new roots (outside a tree, or with nothing, or more than one node, selected).
    None,
}

/// Moves an ink node, keeping its stroke with it: an ink node's points are drawn through its
/// transform, which carries where it is.
pub(super) fn shift_ink(node: &mut Node, by: XY) {
    if node.kind != "ink" || (by.x == 0. && by.y == 0.) {
        return;
    }
    let Some(ink) = node.data.get_mut("ink").and_then(|i| i.as_object_mut()) else {
        return;
    };
    let mut t: Vec<f64> = ink
        .get("transform")
        .and_then(|t| serde_json::from_value(t.clone()).ok())
        .filter(|t: &Vec<f64>| t.len() == 6)
        .unwrap_or_else(|| vec![1., 0., 0., 1., 0., 0.]);
    t[4] += by.x as f64;
    t[5] += by.y as f64;
    ink.insert("transform".into(), serde_json::json!(t));
}

fn hash_of(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// "1 node", "3 nodes".
fn count(n: usize, one: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {one}s")
    }
}

impl Flow {
    /// The selected nodes as a document of their own, as copying puts them on the clipboard:
    /// with what's inside the selected frames and groups, in a tree what's under the selected
    /// nodes, and the edges between them. A node whose parent isn't taken comes out where it
    /// is in the flow, with no parent. None with no node selected (an edge alone isn't copied:
    /// its ends would be missing).
    pub fn selection_document(&self) -> Option<Document> {
        let mut take: HashSet<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.selected)
            .map(|n| n.id.clone())
            .collect();
        if take.is_empty() {
            return None;
        }
        if self.tree_layout().is_some() {
            let tops: Vec<NodeId> = take.iter().cloned().collect();
            for id in &tops {
                take.extend(self.tree.descendants(id));
            }
        }
        // What's inside a taken node goes with it, however deep.
        loop {
            let before = take.len();
            for n in &self.nodes {
                if !take.contains(&n.id) && n.parent.as_ref().is_some_and(|p| take.contains(p)) {
                    take.insert(n.id.clone());
                }
            }
            if take.len() == before {
                break;
            }
        }
        let nodes: Vec<Node> = self
            .nodes
            .iter()
            .filter(|n| take.contains(&n.id))
            .map(|n| {
                let mut n = n.clone();
                n.selected = false;
                if n.parent.as_ref().is_some_and(|p| !take.contains(p)) {
                    let at = self.absolute_position(&n.id).unwrap_or(n.position);
                    let by = at - n.position;
                    shift_ink(&mut n, by);
                    n.position = at;
                    n.parent = None;
                    if n.extent == Some(Extent::Parent) {
                        n.extent = None;
                    }
                }
                n
            })
            .collect();
        let edges: Vec<Edge> = self
            .edges
            .iter()
            .filter(|e| take.contains(&e.source) && take.contains(&e.target))
            .map(|e| Edge {
                selected: false,
                ..e.clone()
            })
            .collect();
        let mut doc = Document::new(nodes, edges);
        doc.layout = self.tree_layout().map(|t| DocLayout::Tree {
            direction: t.direction,
        });
        Some(doc)
    }

    /// ⌘C: puts the selected nodes on the clipboard ([`Flow::selection_document`], as
    /// gpui-flow's JSON). Says whether anything was selected to copy.
    pub fn copy_selected(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.selection_document() else {
            return false;
        };
        let text = doc.to_json();
        // The first paste lands a step along from what was copied.
        self.pastes = Some((hash_of(&text), 0));
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.announce(format!("Copied {}", count(doc.nodes.len(), "node")), cx);
        true
    }

    /// ⌘X: copies the selected nodes, then deletes them as delete does (what may not be
    /// deleted stays). Says whether anything was cut.
    pub fn cut_selected(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.options.deletable {
            return false;
        }
        let Some(doc) = self.selection_document() else {
            return false;
        };
        let text = doc.to_json();
        if !self.delete_selected(cx) {
            return false;
        }
        // What was cut pastes back where it was, the first time.
        self.pastes = Some((hash_of(&text), -1));
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.announce(format!("Cut {}", count(doc.nodes.len(), "node")), cx);
        true
    }

    /// ⌘V: adds what's on the clipboard, if it's gpui-flow's JSON (copied here, from another
    /// flow, or written by anything else), with new ids, selected. Each paste of the same
    /// thing lands a step further along; when that's out of view, it goes in the middle of the
    /// view. In a tree with one node selected, what's pasted goes under it. Says whether there
    /// was anything to paste.
    pub fn paste(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return false;
        };
        let Ok(doc) = Document::from_json(&text) else {
            return false;
        };
        let hash = hash_of(&text);
        let n = match self.pastes {
            Some((h, n)) if h == hash => n + 1,
            _ => 1,
        };
        self.pastes = Some((hash, n));
        let attach = match (self.tree_layout(), self.only_selected()) {
            (Some(_), Some(id)) => Attach::Under(id),
            _ => Attach::None,
        };
        let by = xy(PASTE_STEP * n as f32, PASTE_STEP * n as f32);
        let added = self.insert(doc, by, attach, true, cx);
        if added.is_empty() {
            return false;
        }
        self.announce(format!("Pasted {}", count(added.len(), "node")), cx);
        true
    }

    /// ⌘D: a copy of the selected nodes a step along, selected in their place, without
    /// touching the clipboard. In a tree, each copy is its original's next sibling. Says
    /// whether anything was selected to duplicate.
    pub fn duplicate_selected(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.selection_document() else {
            return false;
        };
        let attach = if self.tree_layout().is_some() {
            Attach::BesideOriginals
        } else {
            Attach::None
        };
        let by = xy(PASTE_STEP, PASTE_STEP);
        let added = self.insert(doc, by, attach, false, cx);
        if added.is_empty() {
            return false;
        }
        self.announce(format!("Duplicated {}", count(added.len(), "node")), cx);
        true
    }

    /// Adds a document's nodes and edges, as a paste does: new ids for all of them (what
    /// pointed at an old one points at its new one), moved by `by`, selected in place of what
    /// was. Gives the new nodes' ids, in the document's order. One edit, so one undo step.
    pub fn insert_document(
        &mut self,
        doc: Document,
        by: XY,
        cx: &mut Context<Self>,
    ) -> Vec<NodeId> {
        self.insert(doc, by, Attach::None, false, cx)
    }

    fn insert(
        &mut self,
        mut doc: Document,
        by: XY,
        attach: Attach,
        keep_in_view: bool,
        cx: &mut Context<Self>,
    ) -> Vec<NodeId> {
        doc.fix_ids();
        if doc.nodes.is_empty() {
            return Vec::new();
        }
        self.edited = true;
        let renamed: HashMap<NodeId, NodeId> = doc
            .nodes
            .iter()
            .map(|n| (n.id.clone(), NodeId::from(json::new_id())))
            .collect();
        // Where it lands: a step along, or, pasted out of view, in the middle of the view.
        let tops: Vec<usize> = (0..doc.nodes.len())
            .filter(|&i| {
                doc.nodes[i]
                    .parent
                    .as_ref()
                    .is_none_or(|p| !renamed.contains_key(p))
            })
            .collect();
        let mut by = by;
        let (pw, ph) = self.pane_size();
        if keep_in_view && self.tree_layout().is_none() && pw > 0. && ph > 0. {
            let bounds = tops
                .iter()
                .map(|&i| {
                    let n = &doc.nodes[i];
                    Rect::new(
                        n.position.x,
                        n.position.y,
                        n.width.unwrap_or(0.),
                        n.height.unwrap_or(0.),
                    )
                })
                .reduce(|a, b| a.union(&b));
            let seen = self.viewport.visible(pw, ph);
            if let Some(b) = bounds {
                let at = Rect::new(b.x + by.x, b.y + by.y, b.w, b.h);
                if !at.intersects(&seen) {
                    by = xy(
                        seen.x + (seen.w - b.w) / 2. - b.x,
                        seen.y + (seen.h - b.h) / 2. - b.y,
                    );
                }
            }
        }
        // In a tree, which pasted nodes are tops: none of the pasted edges come into them.
        let targets: HashSet<&NodeId> = doc
            .edges
            .iter()
            .filter(|e| renamed.contains_key(&e.source))
            .map(|e| &e.target)
            .collect();
        let tree_tops: Vec<NodeId> = doc
            .nodes
            .iter()
            .filter(|n| n.parent.is_none() && !targets.contains(&n.id))
            .map(|n| n.id.clone())
            .collect();
        let mut nodes: Vec<Node> = Vec::with_capacity(doc.nodes.len());
        for (i, mut n) in doc.nodes.into_iter().enumerate() {
            let parent = n.parent.as_ref().and_then(|p| renamed.get(p)).cloned();
            if parent.is_none() {
                if n.parent.is_some() && n.extent == Some(Extent::Parent) {
                    n.extent = None;
                }
                n.parent = None;
            } else {
                n.parent = parent;
            }
            if tops.contains(&i) {
                let mut to = n.position + by;
                if let Some(Extent::Area(_)) = n.extent {
                    to = self.within_extent(&n, to);
                }
                let moved = to - n.position;
                shift_ink(&mut n, moved);
                n.position = to;
            }
            n.id = renamed[&n.id].clone();
            n.selected = true;
            sanitize_node(&mut n);
            nodes.push(n);
        }
        let mut edges: Vec<Edge> = doc
            .edges
            .into_iter()
            .filter_map(|mut e| {
                e.source = renamed.get(&e.source)?.clone();
                e.target = renamed.get(&e.target)?.clone();
                e.id = json::new_id().into();
                e.selected = true;
                sanitize_edge(&mut e);
                Some(e)
            })
            .collect();
        // In a tree, the tops hang from where they're attached, and only they're selected.
        let mut placed_edges: Vec<(usize, Edge)> = Vec::new();
        if self.tree_layout().is_some() {
            let tops_new: HashSet<NodeId> = tree_tops.iter().map(|t| renamed[t].clone()).collect();
            for n in &mut nodes {
                n.selected = tops_new.contains(&n.id);
            }
            edges.iter_mut().for_each(|e| e.selected = false);
            let d = self.options.edge_defaults.clone();
            let hang = |from: NodeId, to: NodeId| Edge {
                id: json::new_id().into(),
                kind: d.kind.clone(),
                animated: d.animated,
                marker_start: d.marker_start,
                marker_end: d.marker_end,
                color: d.color,
                stroke_width: d.stroke_width,
                ..Edge::new("", from, to)
            };
            match &attach {
                Attach::Under(parent) => {
                    self.set_collapsed(parent, false, cx);
                    for t in &tree_tops {
                        placed_edges.push((usize::MAX, hang(parent.clone(), renamed[t].clone())));
                    }
                }
                Attach::BesideOriginals => {
                    for t in &tree_tops {
                        let Some(parent) = self.tree.parent.get(t).cloned() else {
                            continue;
                        };
                        // Siblings are in the order of their edges: right after the original.
                        let at = self
                            .edges
                            .iter()
                            .position(|e| &e.target == t && e.source == parent)
                            .map_or(usize::MAX, |i| i + 1);
                        placed_edges.push((at, hang(parent, renamed[t].clone())));
                    }
                }
                Attach::None => {}
            }
        }
        let ids: Vec<NodeId> = nodes.iter().map(|n| n.id.clone()).collect();
        self.nodes.iter_mut().for_each(|n| n.selected = false);
        self.edges.iter_mut().for_each(|e| e.selected = false);
        self.nodes.extend(nodes);
        self.reindex();
        // Latest place first, so the earlier places stay where they were.
        placed_edges.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (at, edge) in placed_edges {
            let at = at.min(self.edges.len());
            self.edges.insert(at, edge);
        }
        self.edges.extend(edges);
        self.selection_changed(cx);
        cx.notify();
        ids
    }

    /// Delete, as the key does: the selected nodes and edges (or else the one the keyboard is
    /// on), said as it goes. Says whether anything went.
    pub(super) fn delete_and_say(&mut self, cx: &mut Context<Self>) -> bool {
        let before = (self.nodes.len(), self.edges.len());
        if !(self.options.deletable && (self.delete_selected(cx) || self.delete_focused(cx))) {
            return false;
        }
        let gone = (
            before.0.saturating_sub(self.nodes.len()),
            before.1.saturating_sub(self.edges.len()),
        );
        let words = match gone {
            (0, e) => format!("Deleted {}", count(e, "edge")),
            (n, 0) => format!("Deleted {}", count(n, "node")),
            (n, e) => format!("Deleted {} and {}", count(n, "node"), count(e, "edge")),
        };
        self.announce(words, cx);
        true
    }

    /// The edit toolbar's buttons for the selected node ([`FlowOptions::edit_toolbar`]):
    /// Copy, Paste, Duplicate, and Delete when it may be deleted.
    pub(super) fn edit_buttons(
        &self,
        node: &Node,
        this: &WeakEntity<Flow>,
        colors: FlowColors,
    ) -> AnyElement {
        let deletable = self.options.deletable && node.deletable.unwrap_or(true);
        let button = |id: &'static str,
                      label: &'static str,
                      act: fn(&mut Flow, &mut Context<Flow>) -> bool| {
            let this = this.clone();
            Button::new(id)
                .label(label)
                .ghost()
                .with_size(ControlSize::Touch)
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = this.update(cx, act);
                })
        };
        div()
            .debug_selector(|| "flow-edit-toolbar".into())
            .flex()
            .items_center()
            .rounded(px(8.))
            .border_1()
            .border_color(colors.panel_border)
            .bg(colors.panel)
            .child(button("flow-copy", "Copy", Flow::copy_selected))
            .child(button("flow-paste", "Paste", Flow::paste))
            .child(button(
                "flow-duplicate",
                "Duplicate",
                Flow::duplicate_selected,
            ))
            .when(deletable, |d| {
                d.child(button("flow-delete", "Delete", Flow::delete_and_say))
            })
            .into_any_element()
    }
}
