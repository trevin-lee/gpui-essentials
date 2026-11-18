//! Palette drops, on gpui-dnd: a host's list of node kinds dragged onto the canvas, as React
//! Flow's drag-and-drop example does with the browser's.
//!
//! Each of the host's palette entries is a gpui-dnd source carrying a [`FlowPaletteItem`] (a node
//! template), usually [`FlowPaletteItem::source`]. The flow's pane takes it: while it's over the
//! pane the flow draws where it would land, at the canvas's zoom and snap, and a drop sends
//! [`FlowEvent::NodeDropped`](crate::FlowEvent::NodeDropped) with that place, for the host to
//! add the node (`Flow::add_node`) as it likes. The window needs a [`gpui_dnd::DndProvider`].

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ElementId, EntityId, IntoElement, ParentElement as _, SharedString,
    Styled as _, div, px,
};
use gpui_dnd::{DragMonitor, DragSource, Preview};

use crate::theme::FlowColors;
use crate::types::{Node, NodeId, XY};

/// The width a built-in node takes without a size of its own.
const DEFAULT_WIDTH: f32 = 150.;
/// About a built-in node's height with a line of text in it.
const DEFAULT_HEIGHT: f32 = 40.;

/// A node kind from a host's palette, as it's dragged onto a flow: the node to add (its kind,
/// data, size and the rest) and what it's called.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowPaletteItem {
    /// What it's called, in the preview and in what assistive technology is told ("Note").
    pub name: SharedString,
    /// The node it adds. Its id and position are set when it's dropped
    /// ([`node_at`](Self::node_at)).
    pub node: Node,
}

impl FlowPaletteItem {
    pub fn new(name: impl Into<SharedString>, node: Node) -> Self {
        Self {
            name: name.into(),
            node,
        }
    }

    /// How large it lands, in the flow: the node's own size, or a built-in node's width by a
    /// line's height where it has none.
    pub fn size(&self) -> (f32, f32) {
        (
            self.node.width.unwrap_or(DEFAULT_WIDTH),
            self.node.height.unwrap_or(DEFAULT_HEIGHT),
        )
    }

    /// The node to add for a drop at `position` (from
    /// [`NodeDropped`](crate::FlowEvent::NodeDropped)), with its own id.
    pub fn node_at(&self, id: impl Into<NodeId>, position: XY) -> Node {
        Node {
            id: id.into(),
            position,
            ..self.node.clone()
        }
    }

    /// A gpui-dnd source for this item, for a palette entry: `dnd(id).source(item.source())`.
    /// A card with its name follows the pointer, and goes once the pointer is over a flow,
    /// where the flow draws the node in place; dropped anywhere else, it slides back.
    pub fn source(self) -> DragSource<FlowPaletteItem> {
        DragSource::item(self).preview(Preview::build(|items, _, cx| card(items, cx)))
    }
}

/// The preview of a palette item off the canvas: a card of its entry's size with its name.
fn card(items: &[FlowPaletteItem], cx: &mut App) -> AnyElement {
    let monitor = DragMonitor::of(cx);
    if monitor.target_ids().iter().any(is_drop_target) && !monitor.is_returning() {
        return div().into_any_element();
    }
    let colors = FlowColors::from_theme(cx);
    let size = monitor.source_size();
    div()
        .when_some(size, |d, s| d.w(s.width).h(s.height))
        .flex()
        .items_center()
        .px(px(10.))
        .rounded(px(4.))
        .border_1()
        .border_color(colors.selected)
        .bg(colors.node)
        .text_color(colors.node_text)
        .text_size(px(12.))
        .shadow_md()
        .children(items.first().map(|i| i.name.clone()))
        .into_any_element()
}

const DROP_TARGET: &str = "gpui-flow-drop";

/// The id of a flow's pane as a gpui-dnd target.
pub(crate) fn drop_target_id(flow: EntityId) -> ElementId {
    ElementId::NamedInteger(DROP_TARGET.into(), flow.as_u64())
}

/// Whether a gpui-dnd target is a flow's pane.
pub(crate) fn is_drop_target(id: &ElementId) -> bool {
    matches!(id, ElementId::NamedInteger(name, _) if name.as_ref() == DROP_TARGET)
}

/// Where a palette item lands for the pointer at `at` (a flow point): its middle under the
/// pointer, its corner on the grid when snapping is on.
pub(crate) fn landing(item: &FlowPaletteItem, at: XY, snap: impl Fn(XY) -> XY) -> XY {
    let (w, h) = item.size();
    snap(crate::types::xy(at.x - w / 2., at.y - h / 2.))
}
