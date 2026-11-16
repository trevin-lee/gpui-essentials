//! A node-based editor in the manner of React Flow, in GPUI.
//!
//! A [`Flow`] is an entity that holds nodes, edges and a viewport. It draws them on a pane that
//! pans and zooms, and lets people drag nodes, select them one at a time or with a rectangle,
//! connect them by dragging between handles, and delete them. The pieces carry React Flow's
//! names and defaults: node kinds (`default`, `input`, `output`, `group`, and the host's own),
//! [`Handle`]s, edge kinds (bezier, straight, step, smooth step, simple bezier) with labels,
//! arrowheads and animation, a [`FlowBackground`], controls, a minimap and panels.
//!
//! ```ignore
//! gpui_flow::init(cx); // once, after gpui_core::init
//! let flow = cx.new(|cx| {
//!     Flow::new(cx)
//!         .with_nodes(vec![
//!             Node::new("a", xy(0., 0.)).kind("input").label("Start"),
//!             Node::new("b", xy(100., 120.)).label("Then"),
//!         ])
//!         .with_edges(vec![Edge::new("a-b", "a", "b")])
//!         .with_background(FlowBackground::dots())
//!         .with_controls()
//!         .with_minimap()
//!         .fit_view_on_start()
//! });
//! ```
//!
//! Nodes are ordinary GPUI elements made by a renderer per kind ([`Flow::with_node_kind`]).
//! GPUI can't scale an element, so a node follows the zoom by sizing itself in [`units`]
//! (see [`node`]). A child of a node that takes presses or scrolls itself (a button, a text
//! field, a list) is marked [`NodeInput::nodrag`] or [`NodeInput::nowheel`], as React Flow's
//! `nodrag` and `nowheel` classes mark it.
//!
//! A flow saves and loads as React Flow's own JSON ([`FlowObject`]), keeping the fields it
//! doesn't read.

mod flow;
pub mod geometry;
pub mod graph;
pub mod node;
mod paint;
pub mod routing;
pub mod shapes;
mod theme;
pub mod tree;
mod types;

pub use flow::{
    AddChild, AddSibling, Cancel, CanvasTransform, ContextTarget, DeleteSelected, EdgeDefaults,
    FitView, Flow, FlowEvent, FlowOptions, FrameSelection, HandleRef, MoveDown, MoveDownFar,
    MoveLeft, MoveLeftFar, MoveRight, MoveRightFar, MoveUp, MoveUpFar, OpenLink, PanelPosition,
    PointerMode, SelectAll, Unframe, ZoomIn, ZoomOut, ZoomReset, bind_keys,
};
pub use node::{
    HANDLE_SIZE, Handle, NodeContext, NodeInput, NodeRenderer, font_size, frame_node, group_node,
    labeled_node, shape_node, topic_node, units,
};
pub use paint::{BackgroundVariant, FlowBackground};
pub use theme::{FlowColors, Palette, Pencil};
pub use tree::{Layout, Tree, TreeDirection, TreeLayout};
pub use types::*;

/// Sets the flow up: its key bindings. Call once, after `gpui_core::init`.
pub fn init(cx: &mut gpui::App) {
    bind_keys(cx);
}
