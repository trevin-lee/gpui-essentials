//! Connecting two nodes from the keyboard, as clicking one handle and then another does: C on
//! the node the keyboard is on (or the one selected) begins a connection from it, Tab moves
//! the keyboard to another node, and Enter connects to it; Escape lets it go. Each step is
//! said.

use gpui::{Context, SharedString};

use super::{Flow, FlowItem, HandleRef};
use crate::types::*;

impl Flow {
    /// The node a keyboard command is for: the one the keyboard is on, else the one selected.
    fn keyboard_node(&self) -> Option<NodeId> {
        match &self.keyboard_on {
            Some(FlowItem::Node(id)) => Some(id.clone()),
            _ => self.only_selected(),
        }
        .filter(|id| self.node(id).is_some())
    }

    /// A node's handles that take connections, as last drawn: kind, id, and middle in the flow.
    fn connectable_handles(&self, id: &NodeId) -> Vec<(HandleKind, Option<SharedString>, XY)> {
        let Some(at) = self.absolute_position(id) else {
            return Vec::new();
        };
        if !self.node(id).is_some_and(|n| self.connectable(n)) {
            return Vec::new();
        }
        let measured = self.shared.measured.borrow();
        measured.get(id).map_or_else(Vec::new, |m| {
            m.handles
                .iter()
                .filter(|h| h.connectable)
                .map(|h| {
                    let c = h.rect.center();
                    (h.kind, h.id.clone(), xy(at.x + c.x, at.y + c.y))
                })
                .collect()
        })
    }

    /// C: begins a connection from the node the keyboard is on (or the one selected), from its
    /// first handle that connections leave from (else its first). Tab then moves the keyboard
    /// to another node, Enter connects to it, and Escape lets it go. Says whether there was a
    /// node to begin from.
    pub fn start_keyboard_connect(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.keyboard_node() else {
            return false;
        };
        self.let_go_of_click_connect(cx);
        let handles = self.connectable_handles(&id);
        let from = handles
            .iter()
            .find(|h| h.0 == HandleKind::Source)
            .or_else(|| handles.first())
            .cloned();
        let Some((kind, handle, at)) = from else {
            let words = format!("{} can't be connected", self.name_of(&id));
            self.announce(words, cx);
            return true;
        };
        self.click_connect(
            HandleRef {
                node: id.clone(),
                handle,
                kind,
            },
            at,
            cx,
        );
        // On from here, so Tab moves on to the next node.
        self.keyboard_on = Some(FlowItem::Node(id.clone()));
        let words = format!(
            "Connecting from {}. Tab to another node and press Enter to connect, or Escape to stop",
            self.name_of(&id)
        );
        self.announce(words, cx);
        true
    }

    /// The node a keyboard connection is coming from, while one is.
    pub fn keyboard_connecting(&self) -> Option<NodeId> {
        self.shared
            .click_connect
            .borrow()
            .as_ref()
            .map(|h| h.node.clone())
    }

    /// Enter while connecting: connects to the node the keyboard is on, by its handle nearest
    /// the one the connection comes from (of the other kind, unless connections are loose).
    /// Says whether a connection was under way and the keyboard on another node.
    pub(super) fn finish_keyboard_connect(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(from) = self.shared.click_connect.borrow().clone() else {
            return false;
        };
        let Some(FlowItem::Node(to)) = self.keyboard_on.clone() else {
            return false;
        };
        if to == from.node {
            return false;
        }
        let start = self
            .handle_point(&from.node, from.kind, from.handle.as_deref())
            .unwrap_or_default();
        let loose = self.options.connection_mode == ConnectionMode::Loose;
        let target = self
            .connectable_handles(&to)
            .into_iter()
            .filter(|h| loose || h.0 != from.kind)
            .min_by(|a, b| {
                a.2.distance(start)
                    .partial_cmp(&b.2.distance(start))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        let (from_name, to_name) = (self.name_of(&from.node), self.name_of(&to));
        let Some((kind, handle, at)) = target else {
            self.let_go_of_click_connect(cx);
            let words = format!("{to_name} can't be connected to");
            self.announce(words, cx);
            return true;
        };
        let edges = self.edges.len();
        self.click_connect(
            HandleRef {
                node: to.clone(),
                handle,
                kind,
            },
            at,
            cx,
        );
        if self.edges.len() == edges {
            let words = format!("Can't connect {from_name} to {to_name}");
            self.announce(words, cx);
        }
        true
    }
}
