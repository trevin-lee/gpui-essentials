//! Resizing a node from the keyboard, as its resizer's corner does with the mouse: R on the
//! node the keyboard is on (or the one selected) starts it, the arrows grow or shrink it a step
//! (shift for four), Enter keeps the size as one edit, and Escape puts it back. Each step is
//! said ("Start, 250 by 120").

use gpui::{Context, SharedString};

use super::{Flow, FlowEvent, FlowItem};
use crate::types::*;

/// A keyboard resize under way.
pub(super) enum KeyResize {
    /// Asked for on a node that wasn't selected: it is now, and its resizer (the size it may
    /// shrink to, whether it keeps its proportions) is looked for as the frame is drawn.
    Asked(NodeId),
    On {
        node: NodeId,
        /// Its size when the resize began, and the size it was given then, for Escape.
        size: (f32, f32),
        given: (Option<f32>, Option<f32>),
    },
}

/// How much one arrow press changes a size: ten points, or a grid square when snapping.
const STEP: f32 = 10.;

fn rounded((w, h): (f32, f32)) -> String {
    format!("{} by {}", w.round(), h.round())
}

impl Flow {
    /// Starts resizing a node from the keyboard: the node the keyboard is on, else the one
    /// node selected. It must have a resizer (`NodeContext::resizer`), which a node's
    /// renderer draws while it's selected. Says whether there was a node to resize.
    pub fn start_keyboard_resize(&mut self, cx: &mut Context<Self>) -> bool {
        self.end_keyboard_resize(true, cx);
        let target = match &self.keyboard_on {
            Some(FlowItem::Node(id)) => Some(id.clone()),
            _ => self.only_selected(),
        };
        let Some(id) = target.filter(|id| self.node(id).is_some()) else {
            return false;
        };
        if self.only_selected().as_ref() == Some(&id) && self.resizer_of(&id).is_some() {
            self.begin_key_resize(&id, cx);
        } else {
            // Selected alone, it draws its resizer; the frame being drawn says whether it has one.
            self.nodes.iter_mut().for_each(|n| n.selected = n.id == id);
            self.edges.iter_mut().for_each(|e| e.selected = false);
            self.selection_changed(cx);
            self.key_resize = Some(KeyResize::Asked(id));
        }
        cx.notify();
        true
    }

    /// The node being resized from the keyboard, if one is.
    pub fn keyboard_resizing(&self) -> Option<&NodeId> {
        match &self.key_resize {
            Some(KeyResize::On { node, .. }) => Some(node),
            _ => None,
        }
    }

    /// What the resizer drawn for a node in the last frame allows: the least size, and whether
    /// it keeps its proportions.
    fn resizer_of(&self, id: &NodeId) -> Option<crate::node::ResizerInfo> {
        self.shared.resizers.borrow().get(id).copied()
    }

    fn begin_key_resize(&mut self, id: &NodeId, cx: &mut Context<Self>) {
        let (Some(node), Some(size)) = (self.node(id), self.node_size(id)) else {
            self.key_resize = None;
            return;
        };
        let given = (node.width, node.height);
        self.key_resize = Some(KeyResize::On {
            node: id.clone(),
            size,
            given,
        });
        cx.emit(FlowEvent::NodeResizeStart(id.clone()));
        let words = format!(
            "Resizing {}, {}. The arrows change its size, Enter keeps it, Escape puts it back",
            self.name_of(id),
            rounded(size)
        );
        self.announce(words, cx);
    }

    /// While the frame is drawn, after the nodes: a resize asked for on a node that wasn't
    /// selected begins now that its resizer is known, or is said to be impossible.
    pub(super) fn settle_key_resize(&mut self, cx: &mut Context<Self>) {
        let Some(KeyResize::Asked(id)) = &self.key_resize else {
            return;
        };
        let id = id.clone();
        if self.resizer_of(&id).is_some() && self.node_size(&id).is_some() {
            self.begin_key_resize(&id, cx);
        } else {
            self.key_resize = None;
            let words = format!("{} can't be resized", self.name_of(&id));
            self.announce(words, cx);
        }
    }

    /// An arrow while resizing: right and down grow the node a step, left and up shrink it,
    /// its top left corner staying put; four steps with `far`. A node that keeps its
    /// proportions grows both ways. Says whether a resize is under way.
    pub(super) fn key_resize_step(
        &mut self,
        dx: f32,
        dy: f32,
        far: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(KeyResize::On { node, .. }) = &self.key_resize else {
            return false;
        };
        let id = node.clone();
        let Some(((min_w, min_h), keep)) = self.resizer_of(&id) else {
            return true;
        };
        let Some((w0, h0)) = self.node_size(&id) else {
            return true;
        };
        let (sx, sy) = if self.options.snap_to_grid {
            self.options.snap_grid
        } else {
            (STEP, STEP)
        };
        let k = if far { 4. } else { 1. };
        let mut w = (w0 + dx * sx * k).max(min_w);
        let mut h = (h0 + dy * sy * k).max(min_h);
        if keep && w0 > 0. && h0 > 0. {
            let a = w0 / h0;
            if dx != 0. {
                h = w / a;
            } else {
                w = h * a;
            }
            if w < min_w || h < min_h {
                w = min_w.max(min_h * a);
                h = w / a;
            }
        }
        self.edited = true;
        if let Some(i) = self.index_of(&id) {
            self.nodes[i].width = Some(w);
            self.nodes[i].height = Some(h);
        }
        cx.emit(FlowEvent::NodeResize(id.clone()));
        let words = format!("{}, {}", self.name_of(&id), rounded((w, h)));
        self.announce(words, cx);
        cx.notify();
        true
    }

    /// Ends a keyboard resize: Enter keeps the size (one edit, so one undo step), Escape
    /// (`keep` false) puts back the size it had. Says whether one was under way.
    pub fn end_keyboard_resize(&mut self, keep: bool, cx: &mut Context<Self>) -> bool {
        let Some(resize) = self.key_resize.take() else {
            return false;
        };
        let KeyResize::On { node, size, given } = resize else {
            return true;
        };
        self.edited = true;
        let now = self.node_size(&node).unwrap_or(size);
        let name = self.name_of(&node);
        let words: SharedString = if keep {
            format!("{name} resized to {}", rounded(now)).into()
        } else {
            if let Some(i) = self.index_of(&node) {
                self.nodes[i].width = given.0;
                self.nodes[i].height = given.1;
            }
            format!("{name} back to {}", rounded(size)).into()
        };
        cx.emit(FlowEvent::NodeResizeEnd(node));
        self.announce(words, cx);
        cx.notify();
        true
    }
}
