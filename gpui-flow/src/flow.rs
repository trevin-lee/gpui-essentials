//! The flow itself: an entity that holds the nodes, the edges and the viewport, draws them,
//! and turns the mouse and keyboard into moves, selections, connections and deletions.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::geometry::{EdgePath, distance_to_polyline, edge_path, longest_middle, rounded_path};
use crate::graph;
use crate::json::{self, DocLayout, Document, Op};
use crate::node::FastMap;
use crate::node::{
    EdgeLine, FRAME_BORDER, FRAME_TITLE, Measured, NodeContext, NodeInput as _, NodeRenderer,
    Reach, Shared, TouchTarget, Zoomed, built_in, measure_node, touch_target, units,
};
use crate::paint::{self, FlowBackground, Icon};
use crate::theme::FlowColors;
use crate::tree::{self, Layout, Tree};
use crate::types::*;

actions!(
    flow,
    [
        /// Removes the selected nodes and edges.
        DeleteSelected,
        /// Selects every node and edge.
        SelectAll,
        /// Drops what's being dragged, or else the selection.
        Cancel,
        ZoomIn,
        ZoomOut,
        /// Back to 100%.
        ZoomReset,
        /// Shows every node.
        FitView,
        /// Moves the selected nodes a step.
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        /// Asks the host to open the selected node's link.
        OpenLink,
        /// Puts the selected nodes in a frame.
        FrameSelection,
        /// Takes the selected frames away, leaving what was in them.
        Unframe,
        /// In a tree layout, adds a child to the selected node.
        AddChild,
        /// In a tree layout, adds a sibling after the selected node.
        AddSibling,
        /// Moves the selected nodes four steps.
        MoveLeftFar,
        MoveRightFar,
        MoveUpFar,
        MoveDownFar,
        /// Takes back the last edit.
        Undo,
        /// Makes the last edit taken back again.
        Redo,
        /// Moves the keyboard to the node or edge before (Tab moves it to the next).
        FocusPrevious,
        /// Selects the node or edge the keyboard is on.
        SelectFocused,
    ]
);

const CONTEXT: &str = "Flow";

/// The flow's keys. Call once at startup ([`crate::init`] does).
///
/// They act only while the flow itself has the focus, never while something inside a node or
/// a panel does (a text field, say), and a key the flow has nothing to do for (escape with
/// nothing selected, an arrow with nothing to move) goes on to the host. Bind the host's own
/// keys first: GPUI lets a binding without a context that's made after these take the key.
pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", DeleteSelected, c),
        KeyBinding::new("delete", DeleteSelected, c),
        KeyBinding::new("cmd-a", SelectAll, c),
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("cmd-=", ZoomIn, c),
        KeyBinding::new("cmd--", ZoomOut, c),
        KeyBinding::new("cmd-0", ZoomReset, c),
        KeyBinding::new("shift-1", FitView, c),
        KeyBinding::new("left", MoveLeft, c),
        KeyBinding::new("right", MoveRight, c),
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("cmd-enter", OpenLink, c),
        KeyBinding::new("cmd-g", FrameSelection, c),
        KeyBinding::new("cmd-shift-g", Unframe, c),
        KeyBinding::new("tab", AddChild, c),
        KeyBinding::new("enter", AddSibling, c),
        KeyBinding::new("shift-left", MoveLeftFar, c),
        KeyBinding::new("shift-right", MoveRightFar, c),
        KeyBinding::new("shift-up", MoveUpFar, c),
        KeyBinding::new("shift-down", MoveDownFar, c),
        KeyBinding::new("cmd-z", Undo, c),
        KeyBinding::new("cmd-shift-z", Redo, c),
        KeyBinding::new("shift-tab", FocusPrevious, c),
        KeyBinding::new("space", SelectFocused, c),
    ]);
}

/// What a new edge made by dragging between handles starts with.
#[derive(Clone, Debug, Default)]
pub struct EdgeDefaults {
    pub kind: EdgeKind,
    pub animated: bool,
    pub marker_start: Option<Marker>,
    pub marker_end: Option<Marker>,
    pub color: Option<u32>,
    pub stroke_width: Option<f32>,
}

/// How a flow behaves. The defaults are React Flow's, except that nodes wholly off screen
/// aren't drawn (`only_render_visible`) and nodes too small to read are drawn as plain boxes
/// (`simplify_below_zoom`): drawing is what costs in GPUI.
#[derive(Clone, Debug)]
pub struct FlowOptions {
    pub min_zoom: f32,
    pub max_zoom: f32,
    /// Whether nodes can be dragged, unless a node says otherwise.
    pub nodes_draggable: bool,
    /// Whether handles take connections, unless a node says otherwise.
    pub nodes_connectable: bool,
    /// Whether nodes and edges can be selected, unless one says otherwise.
    pub elements_selectable: bool,
    /// Whether the delete key removes what's selected.
    pub deletable: bool,
    /// Dragging the pane moves the viewport.
    pub pan_on_drag: bool,
    /// Scrolling moves the viewport; zooming then takes ⌘ or a pinch.
    pub pan_on_scroll: bool,
    /// How far the viewport moves for a point of scrolling, when scrolling pans.
    pub pan_on_scroll_speed: f32,
    /// Scrolling zooms (when it doesn't pan).
    pub zoom_on_scroll: bool,
    pub zoom_on_pinch: bool,
    pub zoom_on_double_click: bool,
    /// Dragging the pane draws a selection rectangle without holding shift.
    pub selection_on_drag: bool,
    pub selection_mode: SelectionMode,
    pub snap_to_grid: bool,
    pub snap_grid: (f32, f32),
    pub connection_mode: ConnectionMode,
    /// How near a handle (in flow units) a dragged connection snaps to it.
    pub connection_radius: f32,
    /// The line drawn while a connection is dragged.
    pub connection_line: EdgeKind,
    pub edge_defaults: EdgeDefaults,
    /// Selected nodes, and edges to them, are drawn above the rest.
    pub elevate_nodes_on_select: bool,
    /// Selected edges are drawn above nodes.
    pub elevate_edges_on_select: bool,
    /// Leaves out nodes that are wholly off screen.
    pub only_render_visible: bool,
    /// Below this zoom, nodes are drawn as boxes of their size, shape and colors (strokes as
    /// they are) instead of being built; 0 never does. 0.5 by default, where 12-point text is
    /// 6 points high: building the nodes in view there is what costs most.
    pub simplify_below_zoom: f32,
    /// What presses, scrolling and pinches on the canvas do: edit it, only move about it, or
    /// nothing, leaving them to a view laid over it.
    pub pointer_mode: PointerMode,
    /// How large (in points) the area is that a finger can take a handle or a resizer's grip
    /// by, at least: Apple's 44 by default. A finger takes an edge from up to half that away.
    pub touch_target_size: f32,
    /// Whether nodes, handles and resizers have finger-sized targets round them. On by default
    /// on iOS; elsewhere they're built only once a finger has touched the canvas, since each
    /// is an element more for every node in view.
    pub touch_targets: bool,
    /// Where nodes go: where they're put, or laid out as a tree read from the edges (a mind
    /// map). In a tree, tab adds a child to the selected node and enter a sibling, the arrows
    /// move the selection, a node dropped on another becomes its child, and delete takes a
    /// node's subtree with it.
    pub layout: Layout,
    /// Step and smooth step edges go round the nodes in their way, where that's quick to work
    /// out (see `routing`); otherwise they take their usual route.
    pub avoid_nodes: bool,
    /// While nodes are dragged, they line up with the edges and middles of the nodes near
    /// them, and guides show what with. Off by default, as in React Flow.
    pub alignment_guides: bool,
    /// How near (in points on screen) a dragged node must come to lining up for it to.
    pub guide_reach: f32,
    /// How far (in points on screen) a pressed node must move before it counts as a drag.
    pub node_drag_threshold: f32,
    /// How long the viewport takes to get where the controls, a fit or a double click send it.
    pub transition: Duration,
    /// How much room a fit leaves around the nodes: 0.1 shows an area a tenth larger.
    pub fit_padding: f32,
    /// While nodes are dragged, the pointer within 40 points of the pane's edge moves the
    /// viewport that way, the nodes with it, as React Flow's `autoPanOnNodeDrag`. On by default.
    pub auto_pan_on_node_drag: bool,
    /// The same while a connection is dragged (React Flow's `autoPanOnConnect`). On by default.
    pub auto_pan_on_connect: bool,
    /// How fast that moves, in points a sixtieth of a second at the very edge (React Flow's
    /// `autoPanSpeed`, 15 by default); slower the further in the pointer is.
    pub auto_pan_speed: f32,
    /// A click (or tap) on a handle begins a connection, and one on another handle finishes it,
    /// as React Flow's `connectOnClick`: no drag needed, which suits a finger. A click on the
    /// same handle again, on the pane or on a node, or escape, lets it go. On by default.
    pub connect_on_click: bool,
    /// An edge's end can be dragged to another handle, the other end staying put, as React
    /// Flow's `edgesReconnectable`: a press within 10 points of the end (a finger's reach on
    /// touch) takes it. On by default.
    pub edges_reconnectable: bool,
    /// What assistive technology calls the canvas ("Mind map", say); "Canvas" by default.
    /// Each node in it is a group named by [`Node::accessible_name`], and each edge one named
    /// by [`Flow::edge_name`].
    pub aria_label: SharedString,
}

impl Default for FlowOptions {
    fn default() -> Self {
        Self {
            min_zoom: 0.5,
            max_zoom: 2.,
            nodes_draggable: true,
            nodes_connectable: true,
            elements_selectable: true,
            deletable: true,
            pan_on_drag: true,
            pan_on_scroll: false,
            pan_on_scroll_speed: 0.5,
            zoom_on_scroll: true,
            zoom_on_pinch: true,
            zoom_on_double_click: true,
            selection_on_drag: false,
            selection_mode: SelectionMode::Full,
            snap_to_grid: false,
            snap_grid: (15., 15.),
            connection_mode: ConnectionMode::Strict,
            connection_radius: 20.,
            connection_line: EdgeKind::Bezier,
            edge_defaults: EdgeDefaults::default(),
            elevate_nodes_on_select: true,
            elevate_edges_on_select: false,
            only_render_visible: true,
            simplify_below_zoom: 0.5,
            pointer_mode: PointerMode::Edit,
            touch_target_size: 44.,
            touch_targets: cfg!(target_os = "ios"),
            layout: Layout::Free,
            avoid_nodes: false,
            alignment_guides: false,
            guide_reach: 6.,
            node_drag_threshold: 1.,
            transition: Duration::ZERO,
            fit_padding: 0.1,
            auto_pan_on_node_drag: true,
            auto_pan_on_connect: true,
            auto_pan_speed: 15.,
            connect_on_click: true,
            edges_reconnectable: true,
            aria_label: "Canvas".into(),
        }
    }
}

/// What pointer input on the canvas does. A host that lays a view of its own over the canvas (a
/// PencilKit canvas to draw on, say) switches between them as the person switches between
/// drawing and editing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerMode {
    /// Presses select, drag, resize and connect; dragging the pane pans; scrolling and
    /// pinching zoom.
    #[default]
    Edit,
    /// Only moving about: a drag anywhere pans, even one that starts on a node, a handle or an
    /// edge; scrolling, pinching and double clicks zoom. Nothing is selected, moved or
    /// connected. For drawing with a pen on a view over the canvas while fingers move it.
    Navigate,
    /// Nothing: the canvas takes no presses, scrolling or pinches, and lets them go on to
    /// whatever is under or around it. The viewport moves only when the host moves it.
    Off,
}

/// What a press on the canvas does while editing: select and move, draw a stroke, or rub
/// strokes out. (With the pointer mode `Navigate` or `Off`, it does what that says.)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Tool {
    #[default]
    Select,
    /// A press anywhere, on a node too, draws a stroke with this pen; the stroke is added as an
    /// `ink` node when the button comes up.
    Pen(Pen),
    /// A drag rubs out every stroke it touches (a stroke at a time, as PencilKit's object
    /// eraser does): one edit for the whole drag.
    Eraser,
}

/// A pen: its tool, its colour as `0xRRGGBBAA`, and its width in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub tool: crate::ink::InkTool,
    pub color: u32,
    pub width: f32,
}

impl Pen {
    /// A pen of this tool, colour and width; the width is kept within what a stroke may be
    /// drawn at (above 0 and at most [`crate::ink::MAX_WIDTH`]).
    pub fn new(tool: crate::ink::InkTool, color: u32, width: f32) -> Self {
        let width = crate::ink::fit_width(width as f64) as f32;
        Self { tool, color, width }
    }

    /// A ball pen in the palette's colour of this name (black for none or `default`), as
    /// `colors` draws it. The stored colour is fixed: it doesn't follow the theme.
    pub fn palette(key: Option<&str>, width: f32, colors: &FlowColors) -> Self {
        Self::new(crate::ink::InkTool::Ball, colors.ink(key), width)
    }
}

/// How far (in points) the eraser reaches from the pointer.
const ERASER_REACH: f32 = 8.;

/// Where the canvas is in the window and how the flow is scaled in it: what a view laid over
/// the canvas needs to line up with the flow. A flow point `p` is at
/// `pane.origin + p * viewport.zoom + (viewport.x, viewport.y)` in the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasTransform {
    /// The pane's bounds in the window.
    pub pane: Bounds<Pixels>,
    pub viewport: Viewport,
}

impl CanvasTransform {
    /// A flow point as a point in the window.
    pub fn flow_to_window(&self, p: XY) -> Point<Pixels> {
        let at = self.viewport.to_pane(p);
        point(self.pane.origin.x + px(at.x), self.pane.origin.y + px(at.y))
    }

    /// A point in the window as a flow point.
    pub fn window_to_flow(&self, p: Point<Pixels>) -> XY {
        self.viewport.to_flow(xy(
            f32::from(p.x - self.pane.origin.x),
            f32::from(p.y - self.pane.origin.y),
        ))
    }

    /// The scale from flow units to window points.
    pub fn scale(&self) -> f32 {
        self.viewport.zoom
    }

    /// Where the flow's origin is in the window: with [`CanvasTransform::scale`], the affine
    /// transform from flow to window coordinates (scale first, then this translation).
    pub fn translation(&self) -> Point<Pixels> {
        self.flow_to_window(xy(0., 0.))
    }
}

/// What a context menu was asked for on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextTarget {
    Node(NodeId),
    Edge(EdgeId),
    Pane,
}

/// A guide drawn while a drag lines a node up with another: a line in the flow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub from: XY,
    pub to: XY,
}

/// Which edges or middles [`Flow::align_selected`] lines up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    /// The middles across.
    Center,
    Right,
    Top,
    /// The middles up and down.
    Middle,
    Bottom,
}

/// Which way [`Flow::distribute_selected`] spaces nodes out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribute {
    Horizontally,
    Vertically,
}

/// One handle of one node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleRef {
    pub node: NodeId,
    pub handle: Option<SharedString>,
    pub kind: HandleKind,
}

/// What the keyboard is on inside the canvas, which Tab and shift-Tab move: a node or an edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowItem {
    Node(NodeId),
    Edge(EdgeId),
}

/// What a flow tells its subscribers.
///
/// The events say what someone did with the mouse and the keyboard. Changes made by calling
/// the flow's methods aren't reported back, with two exceptions: a [`FlowEvent::SelectionChange`]
/// follows any change to what's selected, and a [`FlowEvent::ViewportChange`] any move of the
/// viewport, whatever made it.
// Events are made one at a time and handed straight to subscribers, so the size of the
// largest one costs nothing worth a box.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum FlowEvent {
    /// Someone dragged from one handle to another, and this edge was added for it.
    Connect(Edge),
    /// A connection is being dragged out of this handle.
    ConnectStart {
        node: NodeId,
        handle: Option<SharedString>,
        kind: HandleKind,
    },
    /// The connection drag that began at `from` ended, at `pointer` in the flow: over the
    /// handle `to` or not, and with an edge made or not. Dropped on the pane, it's where a
    /// host can add a node.
    ConnectEnd {
        from: HandleRef,
        to: Option<HandleRef>,
        connected: bool,
        pointer: XY,
    },
    /// An end of this edge was taken hold of, to move to another handle
    /// ([`FlowOptions::edges_reconnectable`]).
    ReconnectStart(EdgeId),
    /// The edge's end was dropped on a handle it can join: `old` is the edge as it was, `new`
    /// as it is now, with the same id.
    Reconnect {
        old: Edge,
        new: Edge,
    },
    /// The end was let go of, moved or not.
    ReconnectEnd {
        edge: EdgeId,
        reconnected: bool,
    },
    NodeClick(NodeId),
    NodeDoubleClick(NodeId),
    /// The mouse began moving these nodes; [`FlowEvent::NodeDrag`] follows each move, and
    /// [`FlowEvent::NodeDragStop`] the release, or escape (which puts them back first).
    NodeDragStart(Vec<NodeId>),
    NodeDrag(Vec<NodeId>),
    NodeDragStop(Vec<NodeId>),
    /// The arrow keys, or an align or distribute command, moved these nodes.
    NodeMove(Vec<NodeId>),
    /// A node's resizer was taken hold of, moved, and let go of (or escape put it back).
    /// Resizing from a top or left corner moves the node as well.
    NodeResizeStart(NodeId),
    NodeResize(NodeId),
    NodeResizeEnd(NodeId),
    EdgeClick(EdgeId),
    EdgeDoubleClick(EdgeId),
    /// A click on the empty pane, at this flow point.
    PaneClick(XY),
    SelectionChange {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
    },
    /// These were removed with the delete key or [`Flow::delete_selected`].
    Delete {
        nodes: Vec<Node>,
        edges: Vec<Edge>,
    },
    /// The viewport moved or zoomed.
    ViewportChange(Viewport),
    /// The nodes, edges or frames changed: what changed, by id, for the host to store (see
    /// [`json::Op`]). One batch for each edit, sent when it's settled: a drag of many nodes is
    /// one batch, at its end. Edits made through the flow's methods (`update_node`,
    /// `set_nodes`, `connect`, …) are sent as well; loading (`restore`, `set_document`) and
    /// applying edits from elsewhere ([`Flow::apply`]) send nothing. What's selected and where
    /// the flow is in view aren't edits.
    Edits(Vec<Op>),
    /// Someone pressed a node's link button, or ⌘↩ with it selected: the host opens what
    /// `link` names, if it will. The flow never opens anything itself.
    OpenLink {
        node: NodeId,
        link: SharedString,
    },
    /// A node was added by the flow and selected: with tab or enter in a tree layout (where
    /// the host starts editing its text), or a frame made with ⌘G.
    NodeAdd {
        node: NodeId,
        parent: Option<NodeId>,
    },
    /// Someone pressed a link card's open button: the host opens the address, if it will. The
    /// flow never opens anything itself.
    OpenUrl {
        node: NodeId,
        url: SharedString,
    },
    /// A link card for this address has nothing filled in: the host looks the address up and
    /// fills in the card (`update_node`). Asked once for each node and address.
    LinkCardNeeded {
        node: NodeId,
        url: SharedString,
    },
    /// Files were dropped on the canvas, at this flow point: the host decides what they become
    /// (pictures, say).
    FilesDropped {
        paths: Vec<std::path::PathBuf>,
        position: XY,
    },
    /// A node joined a frame (`parent`) or left one (none): dropped in or dragged out, taken
    /// in by a frame dropped over it, or framed or unframed with ⌘G and ⇧⌘G.
    NodeParentChange {
        node: NodeId,
        parent: Option<NodeId>,
    },
    /// In a tree layout, a node was dropped on another and is now its child.
    NodeReparent {
        node: NodeId,
        parent: NodeId,
    },
    /// In a tree layout, what's under a node was folded away or shown again.
    NodeCollapse {
        node: NodeId,
        collapsed: bool,
    },
    /// A right click, or a long press of a finger, on a node, an edge or the pane, at
    /// `position` in the flow and `window_position` in the window: where the host shows its
    /// menu. The flow shows none itself.
    ContextMenu {
        target: ContextTarget,
        position: XY,
        window_position: Point<Pixels>,
    },
}

/// Where a panel sits in the pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

type PanelRenderer = Rc<dyn Fn(&Flow, &mut Window, &mut App) -> AnyElement>;
type ToolbarRenderer = Rc<dyn Fn(&Node, &Flow, &mut Window, &mut App) -> Option<AnyElement>>;

/// Where a node's toolbar sits ([`Flow::with_node_toolbar`]): the side of the node, how it
/// lines up along that side, and how far (in points on screen) from the node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolbarPlace {
    pub side: Side,
    pub align: ToolbarAlign,
    pub offset: f32,
}

impl Default for ToolbarPlace {
    /// Above the node, centred, 10 points off, as React Flow's.
    fn default() -> Self {
        Self {
            side: Side::Top,
            align: ToolbarAlign::Center,
            offset: 10.,
        }
    }
}

/// How a node's toolbar lines up along the node's side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolbarAlign {
    Start,
    #[default]
    Center,
    End,
}
type ConnectionCheck = Rc<dyn Fn(&Connection, &Flow) -> bool>;
type DeleteCheck = Rc<dyn Fn(&[Node], &[Edge], &Flow) -> bool>;
type NewNode = Rc<dyn Fn(Option<&Node>, &Flow) -> Node>;
pub(crate) type TransformListener = Rc<dyn Fn(CanvasTransform, &mut Window, &mut App)>;

/// The handle a connection drag is over.
#[derive(Clone, Debug)]
struct Snap {
    connection: Connection,
    to: HandleRef,
    /// The middle of the handle, where the line ends.
    point: XY,
    position: Side,
    valid: bool,
}

/// What the mouse is in the middle of.
#[derive(Clone, Debug)]
enum Drag {
    Pan {
        start: XY,
        /// Where the pointer was at the last move: each move pans by the difference, so a zoom
        /// in the middle of a pan isn't undone.
        last: XY,
        from: Viewport,
        moved: bool,
        clicks: usize,
    },
    Nodes {
        /// The press in the pane, for the drag threshold.
        start: XY,
        /// The press in the flow: the nodes follow the flow point under the pointer, so they
        /// stay under it when the view zooms mid-drag.
        grab: XY,
        origins: Vec<(NodeId, XY)>,
        moved: bool,
        pressed: NodeId,
        clicks: usize,
        /// Where the pointer last was, in the flow.
        pointer: XY,
    },
    Select {
        start: XY,
        current: XY,
        keep: Vec<NodeId>,
        /// The selection before, for escape to put back.
        before: (Vec<NodeId>, Vec<EdgeId>),
    },
    Connect {
        from: HandleRef,
        at: XY,
        position: Side,
        pointer: XY,
        snap: Option<Snap>,
        started: bool,
        /// The edge whose other end is being moved, from this end, which stays.
        reconnecting: Option<EdgeId>,
    },
    Resize {
        node: NodeId,
        corner: ResizeCorner,
        start: XY,
        position: XY,
        size: (f32, f32),
        /// The size the node was given before, for escape to put back.
        given: (Option<f32>, Option<f32>),
        min: (f32, f32),
        /// Width over height, when the node keeps its proportions.
        aspect: Option<f32>,
        moved: bool,
    },
    Minimap,
    /// Drawing a stroke: its points so far, in the flow, and when it began.
    Draw {
        pen: Pen,
        points: Vec<crate::ink::InkPoint>,
        began: Instant,
    },
    /// Rubbing out: where the pointer was, and what's been rubbed out so far (put back if it's
    /// cancelled).
    Erase {
        last: XY,
        gone: Vec<(usize, Node)>,
        edges: Vec<Edge>,
    },
}

/// A finger the flow took, until it lifts.
struct TouchPress {
    /// The node it took hold of (by itself, a handle or a grip); none on the pane.
    node: Option<NodeId>,
    /// On the pane, the edge under it: a tap selects it, a drag pans.
    edge: Option<EdgeId>,
    /// Where it touched down, in the window.
    start: Point<Pixels>,
    /// Whether it has gone further than a tap may.
    moved: bool,
    /// Shows the node's context menu if the finger stays put long enough.
    hold: Option<Task<()>>,
    /// The context menu was shown: the rest of the touch does nothing.
    held: bool,
}

/// How far (in points) a finger may move and still tap, and how long it must stay put for a
/// context menu: UIKit's.
const TOUCH_SLOP: f32 = 8.;
/// How near (in points) a press must be to an edge's end to take hold of it, as React Flow's
/// reconnect anchors' 10.
const RECONNECT_REACH: f32 = 10.;
const LONG_PRESS: Duration = Duration::from_millis(500);
/// How soon a second tap must follow the first, and how near, to be a double tap.
const DOUBLE_TAP: Duration = Duration::from_millis(350);
const DOUBLE_TAP_SLOP: f32 = 22.;

struct Transition {
    from: Viewport,
    to: Viewport,
    start: Instant,
    duration: Duration,
}

/// A node-and-edge canvas. Create it with [`Flow::new`] inside `cx.new`, add it to a view as a
/// child, and subscribe to its [`FlowEvent`]s.
pub struct Flow {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    viewport: Viewport,
    /// How the flow behaves; change it at any time and notify.
    pub options: FlowOptions,
    node_kinds: HashMap<SharedString, NodeRenderer>,
    colors: Option<FlowColors>,
    background: Option<FlowBackground>,
    controls: bool,
    minimap: bool,
    panels: Vec<(PanelPosition, PanelRenderer)>,
    node_toolbar: Option<(ToolbarPlace, ToolbarRenderer)>,
    valid_connection: Option<ConnectionCheck>,
    delete_check: Option<DeleteCheck>,
    transform_listener: Option<TransformListener>,
    image_loader: Option<crate::node::ImageLoader>,
    /// The link cards already asked about, by node and address.
    asked: HashSet<(NodeId, SharedString)>,
    /// Where each node is in `nodes`, built on the first lookup after the list changes: a
    /// frame looks nodes up by id several times each.
    index: RefCell<Option<FastMap<NodeId, usize>>>,
    shared: Rc<Shared>,
    focus: FocusHandle,
    drag: Option<Drag>,
    /// The node or edge the keyboard is on, with a focus ring round it; none while the
    /// canvas itself has the keyboard, or the mouse was used last.
    keyboard_on: Option<FlowItem>,
    /// What the canvas last said to assistive technology (a node moved, a connection made),
    /// in its live region.
    announcement: SharedString,
    /// Where the pointer of the drag under way last was, in the pane, and when the viewport
    /// last moved for it being near the pane's edge (see [`FlowOptions::auto_pan_on_node_drag`]).
    drag_pane: Option<XY>,
    auto_panned: Option<Instant>,
    /// The button that began the drag: only its release ends it.
    drag_button: MouseButton,
    transition: Option<Transition>,
    /// The padding of a fit that's waiting for the nodes' sizes.
    pending_fit: Option<f32>,
    /// The viewport the last fit gave, while nothing has moved it since: a pane that changes
    /// size (a phone's keyboard coming up) fits again rather than leaving the flow off to one
    /// side. And the pane's size the last frame drew, to see it change.
    fitted: Option<Viewport>,
    drawn_pane: Option<(f32, f32)>,
    /// False until a fit asked for before the first frame has happened; nothing shows until then.
    ready: bool,
    /// Runs while an animated edge is on screen, to redraw the dashes.
    ticker: Option<Task<()>>,
    started: Instant,
    /// When the frame being drawn began, by GPUI's clock.
    frame_time: Instant,
    last_selection: (Vec<NodeId>, Vec<EdgeId>),
    touch: Option<TouchPress>,
    /// What a drag of nodes lines up with, to draw.
    guides: Vec<Guide>,
    /// In a tree layout: the tree as of the last frame, the nodes folded under a collapsed
    /// one, where each node is headed, and when the last step toward there was taken.
    tree: Tree,
    folded: HashSet<NodeId>,
    placed: HashMap<NodeId, tree::Placed>,
    laid_out: Option<Instant>,
    /// Makes a new node for tab or enter in a tree layout.
    new_node: Option<NewNode>,
    /// The last tap: when, where, and on which node (none for the pane), for a double tap.
    last_tap: Option<(Instant, Point<Pixels>, Option<NodeId>)>,
    /// What the last document given had that the flow doesn't use: a layout it doesn't know,
    /// and a newer writer's fields. They're written back with the flow's document.
    doc_layout: Option<serde_json::Value>,
    doc_extra: serde_json::Map<String, serde_json::Value>,
    /// The nodes and edges as last stored (sent as edits, applied, or loaded), unselected:
    /// what an edit is told against.
    stored_nodes: HashMap<NodeId, Node>,
    stored_edges: HashMap<EdgeId, Edge>,
    /// Whether the nodes or edges may have changed since the last look for edits; what only
    /// pans, zooms, hovers or selects leaves it alone, so those cost no look.
    edited: bool,
    /// What a press does while editing.
    tool: Tool,
    /// The batches that take back the edits sent, newest last, and those that make edits
    /// taken back again.
    undo_stack: Vec<Vec<Op>>,
    redo_stack: Vec<Vec<Op>>,
    /// What the next batch sent is.
    replay: Replay,
    _settle: Subscription,
}

/// What a batch of edits is, for the undo history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Replay {
    /// An edit of the person's: it can be taken back, and what was taken back can't be made
    /// again.
    #[default]
    Edit,
    Undo,
    Redo,
}

/// How many edits can be taken back.
const UNDO_LIMIT: usize = 200;

impl EventEmitter<FlowEvent> for Flow {}

impl Focusable for Flow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// How fast an animated edge's dashes run, in flow units a second, and how long each is.
const DASH: f32 = 5.;
const DASH_SPEED: f32 = 20.;
/// How wide (in flow units) the band around an edge's line is that takes a press, as React
/// Flow's `interactionWidth`.
const INTERACTION_WIDTH: f32 = 20.;
/// How many nodes out of view that have never been drawn are drawn (to be measured) a frame.
const MEASURE_PER_FRAME: usize = 300;
/// How far (in points) from a stroke's ink a press still takes it.
const INK_REACH: f32 = 6.;
/// The least distance (in points) a press may be from a line and take it, at any zoom.
const MIN_REACH: f32 = 3.;
/// How far past the pane (in points) edges are still drawn, so lines and arrowheads run off
/// its sides cleanly.
const OVERDRAW: f32 = 50.;

/// Puts right what can't be drawn: positions and sizes that aren't numbers, and sizes below 0.
fn sanitize_node(n: &mut Node) {
    if !(n.position.x.is_finite() && n.position.y.is_finite()) {
        n.position = XY::default();
    }
    let size = |v: Option<f32>| v.filter(|v| v.is_finite() && *v >= 0.);
    n.width = size(n.width);
    n.height = size(n.height);
    if let Some(Extent::Area(r)) = n.extent
        && ![r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite())
    {
        n.extent = None;
    }
}

fn sanitize_edge(e: &mut Edge) {
    let width = |v: Option<f32>| v.filter(|v| v.is_finite() && *v > 0.);
    e.stroke_width = width(e.stroke_width);
    e.interaction_width = width(e.interaction_width);
    for marker in [&mut e.marker_start, &mut e.marker_end]
        .into_iter()
        .flatten()
    {
        if !(marker.size.is_finite() && marker.size > 0.) {
            marker.size = Marker::arrow().size;
        }
    }
}

/// Each node's place in the flow, level and box for a frame, by its index (see `Flow::geo`).
struct Geo {
    at: Vec<XY>,
    z: Vec<i64>,
    rect: Vec<Option<Rect>>,
    /// Whether it's the first node with its id (a later one with the same id isn't drawn).
    first: Vec<bool>,
    /// Whether it has measured itself.
    measured: Vec<bool>,
}

/// Items by id as stored: unselected, and the first of any with the same id.
fn stored<T: Clone + Stored>(items: &[T]) -> HashMap<SharedString, T> {
    let mut map = HashMap::with_capacity(items.len());
    for item in items {
        map.entry(item.key().clone()).or_insert_with(|| {
            let mut i = item.clone();
            i.deselect();
            i
        });
    }
    map
}

/// A node or an edge, as the stored copy sees it.
trait Stored {
    fn key(&self) -> &SharedString;
    fn deselect(&mut self);
    fn is_selected(&self) -> bool;
}

impl Stored for Node {
    fn key(&self) -> &SharedString {
        &self.id
    }
    fn deselect(&mut self) {
        self.selected = false;
    }
    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Stored for Edge {
    fn key(&self) -> &SharedString {
        &self.id
    }
    fn deselect(&mut self) {
        self.selected = false;
    }
    fn is_selected(&self) -> bool {
        self.selected
    }
}

/// Whether `now` is `was` (stored, so unselected) apart from its selection.
fn same_stored<T: Clone + PartialEq + Stored>(was: &T, now: &T) -> bool {
    if now.is_selected() {
        let mut now = now.clone();
        now.deselect();
        was == &now
    } else {
        was == now
    }
}

/// Whether two versions of a node draw alike, wherever they are. A node that changed
/// otherwise is drawn again before anything trusts what it measured.
fn same_look(a: &Node, b: &Node) -> bool {
    a.kind == b.kind
        && a.data == b.data
        && a.width == b.width
        && a.height == b.height
        && a.source_position == b.source_position
        && a.target_position == b.target_position
        && a.selected == b.selected
        && a.hidden == b.hidden
        && a.collapsed == b.collapsed
        && a.connectable == b.connectable
        && a.extra == b.extra
}

impl Flow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            viewport: Viewport::default(),
            options: FlowOptions::default(),
            node_kinds: built_in(),
            colors: None,
            background: None,
            controls: false,
            minimap: false,
            panels: Vec::new(),
            node_toolbar: None,
            valid_connection: None,
            delete_check: None,
            transform_listener: None,
            image_loader: None,
            asked: HashSet::new(),
            index: RefCell::new(None),
            shared: Rc::default(),
            focus: cx.focus_handle(),
            drag: None,
            keyboard_on: None,
            announcement: SharedString::default(),
            drag_pane: None,
            auto_panned: None,
            drag_button: MouseButton::Left,
            transition: None,
            pending_fit: None,
            fitted: None,
            drawn_pane: None,
            ready: true,
            ticker: None,
            started: now(cx),
            frame_time: now(cx),
            last_selection: Default::default(),
            touch: None,
            guides: Vec::new(),
            last_tap: None,
            tree: Tree::default(),
            folded: HashSet::new(),
            placed: HashMap::new(),
            laid_out: None,
            new_node: None,
            doc_layout: None,
            doc_extra: serde_json::Map::new(),
            stored_nodes: HashMap::new(),
            stored_edges: HashMap::new(),
            edited: false,
            tool: Tool::Select,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            replay: Replay::Edit,
            _settle: cx.observe_self(|this, cx| {
                this.settle(cx);
            }),
        }
    }

    // Setting a flow up.

    pub fn with_nodes(mut self, mut nodes: Vec<Node>) -> Self {
        nodes.iter_mut().for_each(sanitize_node);
        self.nodes = nodes;
        self.reindex();
        self.last_selection = (self.selected_nodes(), self.selected_edges());
        self.stored_nodes = stored(&self.nodes);
        self
    }

    pub fn with_edges(mut self, mut edges: Vec<Edge>) -> Self {
        edges.iter_mut().for_each(sanitize_edge);
        self.edges = edges;
        self.last_selection = (self.selected_nodes(), self.selected_edges());
        self.stored_edges = stored(&self.edges);
        self
    }

    pub fn with_viewport(mut self, viewport: Viewport) -> Self {
        self.viewport = self.sane_viewport(viewport);
        self
    }

    pub fn with_options(mut self, change: impl FnOnce(&mut FlowOptions)) -> Self {
        change(&mut self.options);
        self.viewport = self.sane_viewport(self.viewport);
        self
    }

    /// Starts with every node in view. Nothing is shown until the nodes have been measured.
    pub fn fit_view_on_start(mut self) -> Self {
        self.pending_fit = Some(self.options.fit_padding);
        self.ready = false;
        self
    }

    /// The pattern on the pane.
    pub fn with_background(mut self, background: FlowBackground) -> Self {
        self.background = Some(background);
        self
    }

    /// The zoom, fit and lock buttons, bottom left.
    pub fn with_controls(mut self) -> Self {
        self.controls = true;
        self
    }

    /// The overview of the whole flow, bottom right. Pressing or dragging in it moves the view.
    pub fn with_minimap(mut self) -> Self {
        self.minimap = true;
        self
    }

    /// The flow's own colors, in place of the theme's.
    pub fn with_colors(mut self, colors: FlowColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Something of the host's laid over a corner or an edge of the pane. It's drawn while the
    /// flow is, so it reads the flow from the reference it's given, not through the entity.
    pub fn with_panel(
        mut self,
        position: PanelPosition,
        render: impl Fn(&Flow, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.panels.push((position, Rc::new(render)));
        self
    }

    /// A toolbar of the host's beside the selected node, as React Flow's `NodeToolbar`: shown
    /// while exactly one node is selected and nothing is being dragged, where `place` says,
    /// at its size on screen whatever the zoom. `render` gives its content for that node, or
    /// none to show none for it. Presses on it are its own: they don't reach the canvas.
    pub fn with_node_toolbar(
        mut self,
        place: ToolbarPlace,
        render: impl Fn(&Node, &Flow, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self {
        self.node_toolbar = Some((place, Rc::new(render)));
        self
    }

    /// The host's toolbar for the one selected node, placed beside it in the pane.
    fn render_node_toolbar(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let (place, render) = self.node_toolbar.clone()?;
        if self.drag.is_some() {
            return None;
        }
        let mut selected = self.nodes.iter().filter(|n| n.selected && self.shown(n));
        let node = selected.next()?;
        if selected.next().is_some() {
            return None;
        }
        let content = render(node, self, window, cx)?;
        let r = self.node_rect(&node.id)?;
        let v = self.viewport;
        let (a, b) = (v.to_pane(xy(r.x, r.y)), v.to_pane(xy(r.x + r.w, r.y + r.h)));
        let (pw, ph) = self.pane_size();
        let gap = place.offset.max(0.);
        // A box from the node's side to the pane's edge, the toolbar at its end nearest the
        // node, lined up along the node.
        let along = |d: Div| match place.align {
            ToolbarAlign::Start => d.items_start(),
            ToolbarAlign::Center => d.items_center(),
            ToolbarAlign::End => d.items_end(),
        };
        let lane = match place.side {
            Side::Top => along(
                div()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .left(px(a.x))
                    .w(px(b.x - a.x))
                    .top(px(a.y - gap - ph))
                    .h(px(ph)),
            ),
            Side::Bottom => along(
                div()
                    .flex()
                    .flex_col()
                    .justify_start()
                    .left(px(a.x))
                    .w(px(b.x - a.x))
                    .top(px(b.y + gap))
                    .h(px(ph)),
            ),
            Side::Left => along(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .top(px(a.y))
                    .h(px(b.y - a.y))
                    .left(px(a.x - gap - pw))
                    .w(px(pw)),
            ),
            Side::Right => along(
                div()
                    .flex()
                    .flex_row()
                    .justify_start()
                    .top(px(a.y))
                    .h(px(b.y - a.y))
                    .left(px(b.x + gap))
                    .w(px(pw)),
            ),
        };
        Some(
            lane.absolute()
                .child(
                    div()
                        .debug_selector(|| "flow-node-toolbar".into())
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_scroll(|_, _, cx| cx.stop_propagation())
                        .child(content),
                )
                .into_any_element(),
        )
    }

    /// Draws nodes of this `kind`, a new one or a built-in one's replacement.
    ///
    /// The renderer is kept as long as the flow is: one that holds the host's entity strongly,
    /// while the host holds the flow, keeps both alive for good. Capture a `WeakEntity`.
    pub fn with_node_kind(
        mut self,
        kind: impl Into<SharedString>,
        render: impl Fn(&Node, &NodeContext, &Flow, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.node_kinds.insert(kind.into(), Rc::new(render));
        self
    }

    /// Decides whether a connection may be made, while it's dragged and when it's dropped.
    pub fn with_connection_check(
        mut self,
        check: impl Fn(&Connection, &Flow) -> bool + 'static,
    ) -> Self {
        self.valid_connection = Some(Rc::new(check));
        self
    }

    /// Decides whether what the delete key is about to remove may go: the selected nodes and
    /// edges that may be deleted, the children going with them, and the edges they hold. False
    /// keeps everything.
    pub fn with_delete_check(
        mut self,
        check: impl Fn(&[Node], &[Edge], &Flow) -> bool + 'static,
    ) -> Self {
        self.delete_check = Some(Rc::new(check));
        self
    }

    /// Calls `listener` with the canvas's [`CanvasTransform`] on the first frame and on every
    /// frame it changes after that: a pan, a zoom (each step of an animated one), or the pane
    /// moving or changing size in the window. It's called while that frame is drawn, so a native
    /// view kept in line from it moves in the same frame as the canvas. It mustn't change the
    /// flow.
    pub fn with_transform_listener(
        mut self,
        listener: impl Fn(CanvasTransform, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.transform_listener = Some(Rc::new(listener));
        self
    }

    /// Makes the nodes tab and enter add in a tree layout, given the parent (none for a new
    /// root). Its id must be new; the flow places it. Without one, a node labelled "New idea"
    /// is made, of the parent's kind.
    pub fn with_new_node(mut self, make: impl Fn(Option<&Node>, &Flow) -> Node + 'static) -> Self {
        self.new_node = Some(Rc::new(make));
        self
    }

    /// Gives `image` nodes and link cards their pictures: called with a picture's `src` while a
    /// node is drawn, it returns what GPUI should draw, or none. The host decides what a `src`
    /// is and where it comes from; the flow never fetches anything itself.
    pub fn with_image_loader(
        mut self,
        load: impl Fn(&str, &mut Window, &mut App) -> Option<ImageSource> + 'static,
    ) -> Self {
        self.image_loader = Some(Rc::new(load));
        self
    }

    /// Asks the host to open a link card's address. Says whether the node has one.
    pub fn open_url(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        let Some(url) = node
            .card
            .as_ref()
            .map(|c| c.url.clone())
            .filter(|u| !u.is_empty())
        else {
            return false;
        };
        cx.emit(FlowEvent::OpenUrl {
            node: node.id.clone(),
            url,
        });
        true
    }

    /// The canvas's transform as of the last frame drawn.
    pub fn canvas_transform(&self) -> CanvasTransform {
        CanvasTransform {
            pane: self.shared.pane.get(),
            viewport: self.viewport,
        }
    }

    /// Changes what pointer input on the canvas does. A drag under way when it changes is
    /// dropped, as escape drops it.
    pub fn set_pointer_mode(&mut self, mode: PointerMode, cx: &mut Context<Self>) {
        if self.options.pointer_mode != mode {
            self.cancel_drag(cx);
            self.options.pointer_mode = mode;
            cx.notify();
        }
    }

    /// Adds or replaces a node kind after the flow is made.
    pub fn set_node_kind(
        &mut self,
        kind: impl Into<SharedString>,
        render: impl Fn(&Node, &NodeContext, &Flow, &mut Window, &mut App) -> AnyElement + 'static,
        cx: &mut Context<Self>,
    ) {
        self.node_kinds.insert(kind.into(), Rc::new(render));
        cx.notify();
    }

    /// The flow's own colors, or none to follow the theme again.
    pub fn set_colors(&mut self, colors: Option<FlowColors>, cx: &mut Context<Self>) {
        self.colors = colors;
        cx.notify();
    }

    // Reading a flow.

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Where the node with this id is in the list. Of two with one id, the first.
    fn index_of(&self, id: &str) -> Option<usize> {
        let mut index = self.index.borrow_mut();
        let index = index.get_or_insert_with(|| {
            let mut index = FastMap::with_capacity_and_hasher(self.nodes.len(), Default::default());
            for (i, node) in self.nodes.iter().enumerate() {
                index.entry(node.id.clone()).or_insert(i);
            }
            index
        });
        index.get(id).copied()
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.get(self.index_of(id)?)
    }

    /// Forgets where the nodes are, after the list of them changed.
    fn reindex(&mut self) {
        *self.index.get_mut() = None;
    }

    pub fn edge(&self, id: &str) -> Option<&Edge> {
        self.edges.iter().find(|e| e.id.as_ref() == id)
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn selected_nodes(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|n| n.selected)
            .map(|n| n.id.clone())
            .collect()
    }

    pub fn selected_edges(&self) -> Vec<EdgeId> {
        self.edges
            .iter()
            .filter(|e| e.selected)
            .map(|e| e.id.clone())
            .collect()
    }

    /// The whole flow as data.
    pub fn to_object(&self) -> FlowObject {
        FlowObject {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
            viewport: self.viewport,
        }
    }

    /// The flow as a stored document (see [`json`]): its nodes, edges and layout, and where
    /// it's in view. Nothing in it is selected: that's the person's own.
    pub fn document(&self) -> Document {
        let mut doc = Document::new(self.nodes.clone(), self.edges.clone());
        doc.nodes.iter_mut().for_each(|n| n.selected = false);
        doc.edges.iter_mut().for_each(|e| e.selected = false);
        doc.layout = match self.options.layout {
            Layout::Tree(t) => Some(DocLayout::Tree {
                direction: t.direction,
            }),
            Layout::Free => self.doc_layout.clone().map(DocLayout::Other),
        };
        doc.viewport = Some(self.viewport);
        doc.extra = self.doc_extra.clone();
        doc
    }

    /// Gives the flow a stored document, in place of what it holds. What's selected stays
    /// selected where it's still there. A tree layout takes the document's direction and keeps
    /// the flow's own gaps; with none, or one this crate doesn't know, nodes stay where
    /// they're put. The viewport moves to the document's when it has one.
    pub fn set_document(&mut self, doc: Document, cx: &mut Context<Self>) {
        let nodes_on: HashSet<NodeId> = self.selected_nodes().into_iter().collect();
        let edges_on: HashSet<EdgeId> = self.selected_edges().into_iter().collect();
        let Document {
            layout,
            mut nodes,
            mut edges,
            viewport,
            extra,
            ..
        } = doc;
        for n in &mut nodes {
            n.selected = nodes_on.contains(&n.id);
        }
        for e in &mut edges {
            e.selected = edges_on.contains(&e.id);
        }
        self.doc_layout = None;
        self.options.layout = match layout {
            Some(DocLayout::Tree { direction }) => {
                let mut t = self.tree_layout().unwrap_or_default();
                t.direction = direction;
                Layout::Tree(t)
            }
            Some(DocLayout::Other(v)) => {
                self.doc_layout = Some(v);
                Layout::Free
            }
            None => Layout::Free,
        };
        self.doc_extra = extra;
        self.restore(
            FlowObject {
                nodes,
                edges,
                viewport: viewport.unwrap_or(self.viewport),
            },
            cx,
        );
    }

    /// The node's parent, its parent's parent, and so on, nearest first. A chain that loops
    /// is cut where it comes back round, and a node that is its own parent has none.
    fn ancestors<'a>(&'a self, node: &'a Node) -> Vec<&'a Node> {
        let mut chain: Vec<&Node> = Vec::new();
        let mut parent = node.parent.as_ref().and_then(|p| self.node(p));
        while let Some(p) = parent {
            if p.id == node.id || chain.iter().any(|c| c.id == p.id) {
                break;
            }
            chain.push(p);
            parent = p.parent.as_ref().and_then(|q| self.node(q));
        }
        chain
    }

    /// A node's top left corner in the flow, whatever it's a child of.
    pub fn absolute_position(&self, id: &str) -> Option<XY> {
        let node = self.node(id)?;
        Some(
            self.ancestors(node)
                .into_iter()
                .fold(node.position, |at, p| at + p.position),
        )
    }

    /// A node's size: the one it was given, or else the one it measured when last drawn.
    pub fn node_size(&self, id: &str) -> Option<(f32, f32)> {
        let node = self.node(id)?;
        let measured = self.shared.measured.borrow().get(id).and_then(|m| m.size);
        match (node.width, node.height, measured) {
            (Some(w), Some(h), _) => Some((w, h)),
            (w, h, Some(m)) => Some((w.unwrap_or(m.0), h.unwrap_or(m.1))),
            _ => None,
        }
    }

    /// A node's box in the flow, once it has a size.
    pub fn node_rect(&self, id: &str) -> Option<Rect> {
        let at = self.absolute_position(id)?;
        let (w, h) = self.node_size(id)?;
        Some(Rect::new(at.x, at.y, w, h))
    }

    /// The box around every node that isn't hidden. None while any of them is unmeasured.
    pub fn nodes_bounds(&self) -> Option<Rect> {
        let mut all: Option<Rect> = None;
        for node in self.nodes.iter().filter(|n| self.shown(n)) {
            let rect = self.node_rect(&node.id)?;
            all = Some(all.map_or(rect, |a| a.union(&rect)));
        }
        all
    }

    /// Where an edge meets one of a node's handles, in the flow: the first handle of that kind,
    /// or the one with that id. None until the node has been drawn, and for a handle it hasn't.
    pub fn handle_point(&self, node: &str, kind: HandleKind, handle: Option<&str>) -> Option<XY> {
        let at = self.absolute_position(node)?;
        let n = self.node(node)?;
        let handle = handle.map(|h| SharedString::from(h.to_string()));
        endpoint(
            &self.shared.measured.borrow(),
            &n.id,
            at,
            &handle,
            kind,
            self.options.connection_mode == ConnectionMode::Loose,
        )
        .map(|(p, _)| p)
    }

    /// An edge's line as it was last drawn, as points in the flow (its curves flattened). None
    /// for an edge that wasn't drawn: hidden, off screen, or missing a handle.
    pub fn edge_line(&self, id: &str) -> Option<Vec<XY>> {
        let v = self.shared.viewport.get();
        self.shared
            .edges
            .borrow()
            .iter()
            .find(|l| l.id.as_ref() == id)
            .map(|l| l.points.iter().map(|p| v.to_flow(*p)).collect())
    }

    /// Where an edge's label was last drawn, centred, in the flow.
    pub fn edge_label_point(&self, id: &str) -> Option<XY> {
        let v = self.shared.viewport.get();
        self.shared
            .edges
            .borrow()
            .iter()
            .find(|l| l.id.as_ref() == id)
            .and_then(|l| l.label)
            .map(|p| v.to_flow(p))
    }

    /// A point in the window as a flow point.
    pub fn screen_to_flow(&self, position: Point<Pixels>) -> XY {
        self.viewport.to_flow(self.shared.pane_point(position))
    }

    /// A flow point as a point in the window.
    pub fn flow_to_screen(&self, position: XY) -> Point<Pixels> {
        let p = self.viewport.to_pane(position);
        let o = self.shared.pane.get().origin;
        point(o.x + px(p.x), o.y + px(p.y))
    }

    fn pane_size(&self) -> (f32, f32) {
        let s = self.shared.pane.get().size;
        (f32::from(s.width), f32::from(s.height))
    }

    /// Whether a node is drawn: not hidden, and not folded under a collapsed node in a tree.
    pub(crate) fn shown(&self, node: &Node) -> bool {
        !node.hidden && !self.folded.contains(&node.id)
    }

    /// The colors the flow draws with: its own, or the theme's.
    pub(crate) fn colors(&self, cx: &App) -> FlowColors {
        self.colors.unwrap_or_else(|| FlowColors::from_theme(cx))
    }

    fn tree_layout(&self) -> Option<tree::TreeLayout> {
        match self.options.layout {
            Layout::Tree(t) => Some(t),
            Layout::Free => None,
        }
    }

    /// The tree the edges make, as of the last frame (empty unless the layout is a tree).
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Folds away what's under a node in a tree layout, or shows it again.
    pub fn set_collapsed(&mut self, id: &str, collapsed: bool, cx: &mut Context<Self>) {
        self.edited = true;
        let Some(i) = self.index_of(id) else {
            return;
        };
        if self.nodes[i].collapsed == collapsed {
            return;
        }
        self.nodes[i].collapsed = collapsed;
        let node = self.nodes[i].id.clone();
        // What's folded away can't stay selected.
        if collapsed {
            let under: HashSet<NodeId> = self.tree.descendants(&node).into_iter().collect();
            for n in &mut self.nodes {
                if under.contains(&n.id) {
                    n.selected = false;
                }
            }
            self.selection_changed(cx);
        }
        cx.emit(FlowEvent::NodeCollapse { node, collapsed });
        cx.notify();
    }

    /// Lays the tree out for this frame: reads it from the edges, folds what's collapsed, and
    /// moves each node toward its place (at once until every node has a size, and for what's
    /// carried along by a drag). Nodes being dragged are roots of their own until they're
    /// dropped.
    fn lay_out_tree(&mut self, now: Instant, window: &mut Window) {
        let Some(options) = self.tree_layout() else {
            if !self.folded.is_empty() || !self.tree.parent.is_empty() || !self.placed.is_empty() {
                self.folded.clear();
                self.tree = Tree::default();
                self.placed.clear();
                self.laid_out = None;
            }
            return;
        };
        let full = Tree::new(&self.nodes, &self.edges);
        let mut tree = full.clone();
        let dragged: Vec<NodeId> = match &self.drag {
            Some(Drag::Nodes {
                moved: true,
                origins,
                ..
            }) => origins.iter().map(|(id, _)| id.clone()).collect(),
            _ => Vec::new(),
        };
        for id in &dragged {
            if let Some(parent) = tree.parent.remove(id)
                && let Some(kids) = tree.children.get_mut(&parent)
            {
                kids.retain(|k| k != id);
            }
        }
        let collapsed: HashSet<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.collapsed)
            .map(|n| n.id.clone())
            .collect();
        let mut folded = HashSet::new();
        for id in &collapsed {
            folded.extend(full.descendants(id));
        }
        let mut root_ids: Vec<NodeId> = tree.roots.clone();
        root_ids.extend(dragged.iter().cloned());
        let roots: Vec<(NodeId, XY)> = root_ids
            .into_iter()
            .filter(|id| !folded.contains(id))
            .filter_map(|id| Some((id.clone(), self.node(&id)?.position)))
            .collect();
        let placed = {
            let measured = self.shared.measured.borrow();
            let size = |id: &NodeId| {
                let node = self.node(id);
                let m = measured.get(id).and_then(|m| m.size);
                match (node.and_then(|n| n.width), node.and_then(|n| n.height), m) {
                    (Some(w), Some(h), _) => (w, h),
                    (w, h, Some(m)) => (w.unwrap_or(m.0), h.unwrap_or(m.1)),
                    (w, h, None) => (w.unwrap_or(150.), h.unwrap_or(40.)),
                }
            };
            tree::layout(&tree, &roots, &size, &|id| collapsed.contains(id), &options)
        };
        // Carried along by what's dragged: they follow it exactly.
        let mut carried: HashSet<NodeId> = HashSet::new();
        for id in &dragged {
            carried.extend(full.descendants(id));
        }
        let step = match self.laid_out {
            Some(last) if !options.motion.is_zero() => {
                let dt = now.duration_since(last).as_secs_f32();
                // An exponential approach: about 95% of the way in the motion's duration.
                Some(1. - (-3. * dt / options.motion.as_secs_f32()).exp())
            }
            _ => None,
        };
        let mut moving = false;
        for node in &mut self.nodes {
            let Some(place) = placed.get(&node.id) else {
                continue;
            };
            // What's dragged goes where the pointer takes it; a node inside a group, where
            // the group has it.
            if dragged.contains(&node.id) || node.parent.is_some() {
                continue;
            }
            let to = place.position;
            node.position = match step {
                Some(k) if !carried.contains(&node.id) => {
                    let next = node.position + (to - node.position) * k;
                    if next.distance(to) < 0.25 {
                        to
                    } else {
                        moving = true;
                        next
                    }
                }
                _ => to,
            };
        }
        self.tree = full;
        self.folded = folded;
        self.placed = placed;
        // Only once every node has a size is the layout final; until then, nodes jump.
        let sized = {
            let measured = self.shared.measured.borrow();
            self.nodes
                .iter()
                .filter(|n| self.shown(n))
                .all(|n| measured.get(&n.id).is_some_and(|m| m.size.is_some()))
        };
        self.laid_out = sized.then_some(now);
        if moving {
            window.request_animation_frame();
        }
    }

    /// Where a tree edge meets its two nodes: the middle of the parent's side the child is on,
    /// and of the child's side facing back. None for an edge that isn't the tree's.
    fn tree_anchors(&self, edge: &Edge) -> Option<((XY, Side), (XY, Side))> {
        self.tree_layout()?;
        if self.tree.parent.get(&edge.target) != Some(&edge.source) {
            return None;
        }
        let side = self.placed.get(&edge.target)?.out;
        let (s, t) = (self.node_rect(&edge.source)?, self.node_rect(&edge.target)?);
        let mid = |r: Rect, side: Side| match side {
            Side::Left => xy(r.x, r.y + r.h / 2.),
            Side::Right => xy(r.right(), r.y + r.h / 2.),
            Side::Top => xy(r.x + r.w / 2., r.y),
            Side::Bottom => xy(r.x + r.w / 2., r.bottom()),
        };
        Some((
            (mid(s, side), side),
            (mid(t, side.opposite()), side.opposite()),
        ))
    }

    /// Adds a node under `parent` (a new root for none), after `after` among its siblings when
    /// given, selects it, and tells the host so it can start editing it.
    fn add_tree_node(
        &mut self,
        parent: Option<NodeId>,
        after: Option<NodeId>,
        cx: &mut Context<Self>,
    ) {
        self.edited = true;
        let parent_node = parent.as_ref().and_then(|p| self.node(p)).cloned();
        let mut node = match &self.new_node {
            Some(make) => make(parent_node.as_ref(), self),
            None => {
                let kind = parent_node
                    .as_ref()
                    .map_or(SharedString::from("default"), |p| p.kind.clone());
                Node::new(json::new_id(), XY::default())
                    .kind(kind)
                    .label("New idea")
            }
        };
        if self.node(&node.id).is_some() {
            return;
        }
        // It grows out of its parent.
        if let Some(p) = &parent_node {
            node.position = p.position;
        }
        node.selected = true;
        sanitize_node(&mut node);
        let id = node.id.clone();
        self.nodes.iter_mut().for_each(|n| n.selected = false);
        self.edges.iter_mut().for_each(|e| e.selected = false);
        self.nodes.push(node);
        self.reindex();
        if let Some(parent) = &parent {
            let d = &self.options.edge_defaults;
            let edge = Edge {
                id: json::new_id().into(),
                kind: d.kind.clone(),
                animated: d.animated,
                marker_start: d.marker_start,
                marker_end: d.marker_end,
                color: d.color,
                stroke_width: d.stroke_width,
                ..Edge::new("", parent.clone(), id.clone())
            };
            // Siblings are in the order of their edges.
            let at = after
                .and_then(|a| {
                    self.edges
                        .iter()
                        .position(|e| e.target == a && &e.source == parent)
                })
                .map_or(self.edges.len(), |i| i + 1);
            self.edges.insert(at, edge);
        }
        cx.emit(FlowEvent::NodeAdd { node: id, parent });
        self.selection_changed(cx);
        cx.notify();
    }

    /// Asks the host to open a node's link. Says whether it has one.
    pub fn open_link(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        let Some(link) = node.link.clone() else {
            return false;
        };
        cx.emit(FlowEvent::OpenLink {
            node: node.id.clone(),
            link,
        });
        true
    }

    /// The one selected node, if exactly one is.
    fn only_selected(&self) -> Option<NodeId> {
        let mut selected = self.nodes.iter().filter(|n| n.selected);
        let first = selected.next()?;
        selected.next().is_none().then(|| first.id.clone())
    }

    /// Tab in a tree: a child of the selected node. Says whether it was the flow's to do.
    fn add_child(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tree_layout().is_none() {
            return false;
        }
        let Some(parent) = self.only_selected() else {
            return false;
        };
        // A collapsed parent opens to show its new child.
        self.set_collapsed(&parent, false, cx);
        self.add_tree_node(Some(parent), None, cx);
        true
    }

    /// Enter in a tree: a sibling after the selected node (a child, under a root).
    fn add_sibling(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tree_layout().is_none() {
            return false;
        }
        let Some(selected) = self.only_selected() else {
            return false;
        };
        match self.tree.parent.get(&selected).cloned() {
            Some(parent) => self.add_tree_node(Some(parent), Some(selected), cx),
            None => self.add_tree_node(Some(selected), None, cx),
        }
        true
    }

    /// An arrow in a tree: the selection moves to the nearest node that way.
    fn select_toward(&mut self, dir: XY, cx: &mut Context<Self>) -> bool {
        if self.tree_layout().is_none() {
            return false;
        }
        let rects: Vec<(NodeId, Rect)> = self
            .nodes
            .iter()
            .filter(|n| self.shown(n) && self.selectable(n))
            .filter_map(|n| Some((n.id.clone(), self.node_rect(&n.id)?)))
            .collect();
        let next = match self.only_selected().and_then(|id| self.node_rect(&id)) {
            Some(from) => tree::nearest_in_direction(&from, dir, &rects),
            None => self.tree.roots.first().cloned(),
        };
        let Some(next) = next else {
            return true;
        };
        self.nodes
            .iter_mut()
            .for_each(|n| n.selected = n.id == next);
        self.edges.iter_mut().for_each(|e| e.selected = false);
        self.selection_changed(cx);
        cx.notify();
        true
    }

    /// The node a drag of `dragged` would be dropped on at `at`, in a tree: the topmost one
    /// there that isn't being dragged or under what is.
    fn drop_target(&self, dragged: &[NodeId], at: XY) -> Option<NodeId> {
        self.tree_layout()?;
        self.draw_order()
            .into_iter()
            .rev()
            .map(|(_, i)| &self.nodes[i])
            .filter(|n| !dragged.iter().any(|d| self.tree.is_ancestor(d, &n.id)))
            .find(|n| self.node_rect(&n.id).is_some_and(|r| r.contains(at)))
            .map(|n| n.id.clone())
    }

    /// Makes `node` a child of `parent` in the tree: the edge into it now comes from there.
    fn reparent(&mut self, node: &NodeId, parent: &NodeId, cx: &mut Context<Self>) {
        self.edited = true;
        if node == parent || self.tree.parent.get(node) == Some(parent) {
            return;
        }
        let old = self.tree.parent.get(node).cloned();
        let found = old.as_ref().and_then(|old| {
            self.edges
                .iter()
                .position(|e| &e.target == node && &e.source == old)
        });
        match found {
            Some(i) => {
                let edge = &mut self.edges[i];
                edge.source = parent.clone();
                edge.source_handle = None;
            }
            None => {
                let edge = Edge::new(json::new_id(), parent.clone(), node.clone());
                self.edges.push(edge);
            }
        }
        cx.emit(FlowEvent::NodeReparent {
            node: node.clone(),
            parent: parent.clone(),
        });
    }

    fn draggable(&self, node: &Node) -> bool {
        node.draggable.unwrap_or(self.options.nodes_draggable)
    }

    fn selectable(&self, node: &Node) -> bool {
        node.selectable.unwrap_or(self.options.elements_selectable)
    }

    fn connectable(&self, node: &Node) -> bool {
        node.connectable.unwrap_or(self.options.nodes_connectable)
    }

    /// Whether an ancestor of the node moves in this drag, carrying it: one that's selected
    /// (or pressed) and may be dragged.
    fn carried(&self, node: &Node, pressed: Option<&NodeId>) -> bool {
        self.ancestors(node)
            .into_iter()
            .any(|a| (a.selected || Some(&a.id) == pressed) && self.draggable(a))
    }

    /// A node's own level, before its parents': a frame is a level below what isn't, so what
    /// it's over can still be pressed.
    fn own_z(&self, n: &Node) -> i64 {
        n.z_index as i64 - i64::from(is_frame(n))
            + if self.options.elevate_nodes_on_select && n.selected {
                1000
            } else {
                0
            }
    }

    /// Every node's place in the flow, level and box, worked out together in one pass (each
    /// from its parent's), for a frame: what [`Flow::absolute_position`], [`Flow::node_z`]
    /// and [`Flow::node_rect`] give, without each one walking up its parents.
    fn geo(&self) -> Geo {
        let n = self.nodes.len();
        let parent: Vec<Option<usize>> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                node.parent
                    .as_ref()
                    .and_then(|p| self.index_of(p))
                    .filter(|&p| p != i)
            })
            .collect();
        let mut geo = Geo {
            at: vec![XY::default(); n],
            z: vec![0; n],
            rect: vec![None; n],
            first: vec![false; n],
            measured: vec![false; n],
        };
        // 0: not yet, 1: on the way up, 2: done.
        let mut state = vec![0u8; n];
        let mut chain = Vec::new();
        for i in 0..n {
            if state[i] == 2 {
                continue;
            }
            chain.clear();
            let mut k = Some(i);
            let mut looped = false;
            while let Some(j) = k {
                match state[j] {
                    2 => break,
                    1 => {
                        looped = true;
                        break;
                    }
                    _ => {
                        state[j] = 1;
                        chain.push(j);
                        k = parent[j];
                    }
                }
            }
            for &j in chain.iter().rev() {
                let node = &self.nodes[j];
                if looped {
                    // Round a loop each node is reckoned on its own, as its parents cut it.
                    geo.at[j] = self.absolute_position(&node.id).unwrap_or(node.position);
                    geo.z[j] = self.node_z(node);
                } else {
                    let own = self.own_z(node);
                    match parent[j] {
                        Some(p) => {
                            geo.at[j] = geo.at[p] + node.position;
                            geo.z[j] = if geo.z[p] >= own { geo.z[p] + 1 } else { own };
                        }
                        None => {
                            geo.at[j] = node.position;
                            geo.z[j] = own;
                        }
                    }
                }
                state[j] = 2;
            }
        }
        let measured = self.shared.measured.borrow();
        for (i, node) in self.nodes.iter().enumerate() {
            geo.first[i] = self.index_of(&node.id) == Some(i);
            let m = measured.get(&node.id).and_then(|m| m.size);
            geo.measured[i] = m.is_some();
            let size = match (node.width, node.height, m) {
                (Some(w), Some(h), _) => Some((w, h)),
                (w, h, Some(m)) => Some((w.unwrap_or(m.0), h.unwrap_or(m.1))),
                _ => None,
            };
            geo.rect[i] = size.map(|(w, h)| Rect::new(geo.at[i].x, geo.at[i].y, w, h));
        }
        geo
    }

    /// The level a node is drawn at, as React Flow reckons it: its own `z_index`, raised by
    /// 1000 while it's selected, and always above its parent's.
    fn node_z(&self, node: &Node) -> i64 {
        let own = |n: &Node| self.own_z(n);
        let mut z: Option<i64> = None;
        for n in self
            .ancestors(node)
            .into_iter()
            .rev()
            .chain(std::iter::once(node))
        {
            let mine = own(n);
            z = Some(match z {
                Some(parent) if parent >= mine => parent + 1,
                _ => mine,
            });
        }
        z.unwrap_or(0)
    }

    /// The level an edge is drawn at: its own `z_index`, raised by 1000 while selected when
    /// edges are elevated, and up to the level of a child or selected node at either end.
    fn edge_z(&self, edge: &Edge, ends: [(&Node, i64); 2]) -> i64 {
        let o = &self.options;
        let own = edge.z_index as i64
            + if o.elevate_edges_on_select && edge.selected {
                1000
            } else {
                0
            };
        let lifted = |n: &Node| n.parent.is_some() || (o.elevate_nodes_on_select && n.selected);
        let ends = ends
            .into_iter()
            .filter(|(n, _)| lifted(n))
            .map(|(_, z)| z)
            .max()
            .unwrap_or(0);
        own + ends
    }

    // Changing a flow.

    pub fn set_nodes(&mut self, mut nodes: Vec<Node>, cx: &mut Context<Self>) {
        self.edited = true;
        nodes.iter_mut().for_each(sanitize_node);
        let old = std::mem::replace(&mut self.nodes, nodes);
        self.reindex();
        self.forget_changed(&old);
        self.forget_removed();
        self.selection_changed(cx);
        cx.notify();
    }

    pub fn set_edges(&mut self, mut edges: Vec<Edge>, cx: &mut Context<Self>) {
        self.edited = true;
        edges.iter_mut().for_each(sanitize_edge);
        self.edges = edges;
        self.selection_changed(cx);
        cx.notify();
    }

    pub fn add_node(&mut self, mut node: Node, cx: &mut Context<Self>) {
        self.edited = true;
        sanitize_node(&mut node);
        self.nodes.push(node);
        self.reindex();
        self.selection_changed(cx);
        cx.notify();
    }

    /// Adds the edge unless one already joins the same two handles; says whether it did.
    pub fn add_edge(&mut self, mut edge: Edge, cx: &mut Context<Self>) -> bool {
        self.edited = true;
        sanitize_edge(&mut edge);
        let added = graph::add_edge(edge, &mut self.edges);
        self.selection_changed(cx);
        cx.notify();
        added
    }

    /// Changes one node in place. Says whether there was such a node.
    pub fn update_node(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut Node),
        cx: &mut Context<Self>,
    ) -> bool {
        self.edited = true;
        let Some(i) = self.index_of(id) else {
            return false;
        };
        let before = self.nodes[i].clone();
        change(&mut self.nodes[i]);
        sanitize_node(&mut self.nodes[i]);
        if self.nodes[i].id != before.id {
            self.reindex();
            self.forget_removed();
        } else if !same_look(&before, &self.nodes[i]) {
            self.shared.measured.borrow_mut().remove(&before.id);
        }
        self.selection_changed(cx);
        cx.notify();
        true
    }

    /// Changes one edge in place. Says whether there was such an edge.
    pub fn update_edge(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut Edge),
        cx: &mut Context<Self>,
    ) -> bool {
        self.edited = true;
        let Some(edge) = self.edges.iter_mut().find(|e| e.id.as_ref() == id) else {
            return false;
        };
        change(edge);
        sanitize_edge(edge);
        self.selection_changed(cx);
        cx.notify();
        true
    }

    /// Removes these nodes, the nodes inside them, and every edge that touched any of them.
    pub fn remove_nodes(
        &mut self,
        ids: &[NodeId],
        cx: &mut Context<Self>,
    ) -> (Vec<Node>, Vec<Edge>) {
        self.edited = true;
        let mut gone: HashSet<NodeId> = ids.iter().cloned().collect();
        // Children go with their parents, however deep.
        loop {
            let before = gone.len();
            for node in &self.nodes {
                if node.parent.as_ref().is_some_and(|p| gone.contains(p)) {
                    gone.insert(node.id.clone());
                }
            }
            if gone.len() == before {
                break;
            }
        }
        let (removed, kept): (Vec<Node>, Vec<Node>) = std::mem::take(&mut self.nodes)
            .into_iter()
            .partition(|n| gone.contains(&n.id));
        self.nodes = kept;
        self.reindex();
        let (cut, kept): (Vec<Edge>, Vec<Edge>) = std::mem::take(&mut self.edges)
            .into_iter()
            .partition(|e| gone.contains(&e.source) || gone.contains(&e.target));
        self.edges = kept;
        self.forget_removed();
        self.selection_changed(cx);
        cx.notify();
        (removed, cut)
    }

    pub fn remove_edges(&mut self, ids: &[EdgeId], cx: &mut Context<Self>) -> Vec<Edge> {
        self.edited = true;
        let (removed, kept): (Vec<Edge>, Vec<Edge>) = std::mem::take(&mut self.edges)
            .into_iter()
            .partition(|e| ids.contains(&e.id));
        self.edges = kept;
        self.selection_changed(cx);
        cx.notify();
        removed
    }

    /// Replaces the nodes, the edges and the viewport with saved ones.
    pub fn restore(&mut self, object: FlowObject, cx: &mut Context<Self>) {
        let FlowObject {
            mut nodes,
            mut edges,
            viewport,
        } = object;
        nodes.iter_mut().for_each(sanitize_node);
        edges.iter_mut().for_each(sanitize_edge);
        let old = std::mem::replace(&mut self.nodes, nodes);
        self.reindex();
        self.edges = edges;
        self.stored_nodes = stored(&self.nodes);
        self.stored_edges = stored(&self.edges);
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.transition = None;
        self.drag = None;
        self.forget_changed(&old);
        self.forget_removed();
        self.selection_changed(cx);
        let viewport = self.sane_viewport(viewport);
        self.move_viewport(viewport, cx);
        cx.notify();
    }

    /// Once an edit has settled (it isn't in the middle of a drag or a resize), tells the host
    /// what it changed.
    /// Says whether there was anything to send.
    fn settle(&mut self, cx: &mut Context<Self>) -> bool {
        // Debug builds always look, to catch a change that didn't mark the flow edited.
        if !self.edited && !cfg!(debug_assertions) {
            return false;
        }
        if matches!(
            self.drag,
            Some(Drag::Nodes { .. } | Drag::Resize { .. } | Drag::Erase { .. })
        ) {
            return false;
        }
        self.follow_ink();
        let ops = self.unsent();
        debug_assert!(
            self.edited || ops.is_empty(),
            "a change didn't mark the flow edited: {ops:?}"
        );
        self.edited = false;
        if ops.is_empty() {
            return false;
        }
        // What takes the batch back: what was added goes, what went comes back, and each
        // patched field is put back as it was.
        let (mut back_nodes, mut back_edges, mut back_patches) = (vec![], vec![], vec![]);
        let (mut drop_nodes, mut drop_edges) = (vec![], vec![]);
        for op in &ops {
            match op {
                Op::AddNode { node } => {
                    self.stored_nodes.insert(node.id.clone(), (**node).clone());
                    drop_nodes.push(Op::RemoveNode {
                        id: node.id.clone(),
                    });
                }
                Op::PatchNode { id, patch } => {
                    if let Some(was) = self.stored_nodes.get_mut(id)
                        && let Some(now) = json::patched(was, patch)
                    {
                        let back = json::diff_stored(&now, was);
                        if !back.is_empty() {
                            back_patches.push(Op::PatchNode {
                                id: id.clone(),
                                patch: back,
                            });
                        }
                        *was = now;
                    }
                }
                Op::RemoveNode { id } => {
                    if let Some(was) = self.stored_nodes.remove(id) {
                        back_nodes.push(Op::AddNode {
                            node: Box::new(was),
                        });
                    }
                }
                Op::AddEdge { edge } => {
                    self.stored_edges.insert(edge.id.clone(), (**edge).clone());
                    drop_edges.push(Op::RemoveEdge {
                        id: edge.id.clone(),
                    });
                }
                Op::PatchEdge { id, patch } => {
                    if let Some(was) = self.stored_edges.get_mut(id)
                        && let Some(now) = json::patched(was, patch)
                    {
                        let back = json::diff_stored(&now, was);
                        if !back.is_empty() {
                            back_patches.push(Op::PatchEdge {
                                id: id.clone(),
                                patch: back,
                            });
                        }
                        *was = now;
                    }
                }
                Op::RemoveEdge { id } => {
                    if let Some(was) = self.stored_edges.remove(id) {
                        back_edges.push(Op::AddEdge {
                            edge: Box::new(was),
                        });
                    }
                }
            }
        }
        let mut back = back_nodes;
        back.extend(back_edges);
        back.extend(back_patches);
        back.extend(drop_edges);
        back.extend(drop_nodes);
        match self.replay {
            Replay::Edit => {
                self.redo_stack.clear();
                self.undo_stack.push(back);
                if self.undo_stack.len() > UNDO_LIMIT {
                    self.undo_stack.remove(0);
                }
            }
            Replay::Undo => self.redo_stack.push(back),
            Replay::Redo => self.undo_stack.push(back),
        }
        cx.emit(FlowEvent::Edits(ops));
        true
    }

    /// Takes back the last edit: the person's own, as sent (a drag of many nodes, a deletion,
    /// whatever one call of the flow's changed), and sends what that changes. Each field is
    /// put back as it was before the edit; edits from elsewhere aren't taken back, and one to
    /// a node that's since gone is skipped. Says whether there was an edit to take back.
    pub fn undo(&mut self, cx: &mut Context<Self>) -> bool {
        self.step(Replay::Undo, cx)
    }

    /// Makes the last edit taken back again. Any new edit forgets what was taken back.
    pub fn redo(&mut self, cx: &mut Context<Self>) -> bool {
        self.step(Replay::Redo, cx)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    fn step(&mut self, replay: Replay, cx: &mut Context<Self>) -> bool {
        if self.drag.is_some() {
            return false;
        }
        // An edit not yet sent is sent first, as an edit of its own.
        self.settle(cx);
        loop {
            let stack = match replay {
                Replay::Undo => &mut self.undo_stack,
                _ => &mut self.redo_stack,
            };
            let Some(ops) = stack.pop() else {
                return false;
            };
            self.apply_ops(&ops, false, cx);
            self.edited = true;
            self.replay = replay;
            let sent = self.settle(cx);
            self.replay = Replay::Edit;
            // One that changes nothing now (what it touched is gone) is passed over.
            if sent {
                return true;
            }
        }
    }

    /// Strokes moved here (dragged, nudged, framed) move by their transform too, as the iPad
    /// moves them, so their points never change. One whose transform changed as well (an undo,
    /// someone else's edit) is left as it is.
    fn follow_ink(&mut self) {
        for node in &mut self.nodes {
            if node.kind != "ink" {
                continue;
            }
            let Some(was) = self.stored_nodes.get(&node.id) else {
                continue;
            };
            let d = node.position - was.position;
            let transform = |n: &Node| n.data.get("ink").and_then(|i| i.get("transform")).cloned();
            if (d.x == 0. && d.y == 0.) || transform(node) != transform(was) {
                continue;
            }
            let Some(ink) = node.data.get_mut("ink").and_then(|i| i.as_object_mut()) else {
                continue;
            };
            let mut t: Vec<f64> = ink
                .get("transform")
                .and_then(|t| serde_json::from_value(t.clone()).ok())
                .filter(|t: &Vec<f64>| t.len() == 6)
                .unwrap_or_else(|| vec![1., 0., 0., 1., 0., 0.]);
            t[4] += d.x as f64;
            t[5] += d.y as f64;
            ink.insert("transform".into(), serde_json::json!(t));
        }
    }

    /// What changed since the nodes and edges were last stored: adds, then patches, then
    /// removals, nodes before edges. In a tree layout, where a node under a root is is the
    /// layout's, not an edit.
    fn unsent(&self) -> Vec<Op> {
        let tree = self.tree_layout().is_some();
        let placed = |n: &Node| tree && self.tree.parent.contains_key(&n.id);
        let mut adds = Vec::new();
        let mut patches = Vec::new();
        let mut removes = Vec::new();
        // A node or edge whose id an earlier one has can't be told apart by it: only the first
        // is stored, and only distinct ids are counted.
        let mut distinct = 0;
        for (i, node) in self.nodes.iter().enumerate() {
            if self.index_of(&node.id) != Some(i) {
                continue;
            }
            distinct += 1;
            let Some(was) = self.stored_nodes.get(&node.id) else {
                let mut node = node.clone();
                node.selected = false;
                adds.push(Op::AddNode {
                    node: Box::new(node),
                });
                continue;
            };
            let same = if placed(node) {
                let mut now = node.clone();
                now.position = was.position;
                same_stored(was, &now)
            } else {
                same_stored(was, node)
            };
            if !same {
                let mut patch = json::diff_stored(was, node);
                if placed(node) {
                    patch.remove("position");
                }
                if !patch.is_empty() {
                    patches.push(Op::PatchNode {
                        id: node.id.clone(),
                        patch,
                    });
                }
            }
        }
        if self.stored_nodes.len() + adds.len() != distinct {
            let mut gone: Vec<&NodeId> = self
                .stored_nodes
                .keys()
                .filter(|id| self.index_of(id).is_none())
                .collect();
            gone.sort();
            removes.extend(gone.into_iter().map(|id| Op::RemoveNode { id: id.clone() }));
        }
        let mut edge_adds = 0;
        let mut seen: HashSet<&EdgeId> = HashSet::with_capacity(self.edges.len());
        for edge in &self.edges {
            if !seen.insert(&edge.id) {
                continue;
            }
            let Some(was) = self.stored_edges.get(&edge.id) else {
                let mut edge = edge.clone();
                edge.selected = false;
                adds.push(Op::AddEdge {
                    edge: Box::new(edge),
                });
                edge_adds += 1;
                continue;
            };
            if !same_stored(was, edge) {
                let patch = json::diff_stored(was, edge);
                if !patch.is_empty() {
                    patches.push(Op::PatchEdge {
                        id: edge.id.clone(),
                        patch,
                    });
                }
            }
        }
        if self.stored_edges.len() + edge_adds != seen.len() {
            let here: HashSet<&EdgeId> = self.edges.iter().map(|e| &e.id).collect();
            let mut gone: Vec<&EdgeId> = self
                .stored_edges
                .keys()
                .filter(|id| !here.contains(id))
                .collect();
            gone.sort();
            // Edges go before the nodes they joined.
            let edges_gone: Vec<Op> = gone
                .into_iter()
                .map(|id| Op::RemoveEdge { id: id.clone() })
                .collect();
            removes.splice(0..0, edges_gone);
        }
        adds.sort_by_key(|op| matches!(op, Op::AddEdge { .. }));
        adds.extend(patches);
        adds.extend(removes);
        adds
    }

    /// Applies edits from elsewhere: another person's, an agent's, or the server's (see
    /// [`json::Op`]). Nothing is sent back for them, and what's selected stays selected where
    /// it's still there. An add of an id that's here replaces it, and a patch or removal of one
    /// that isn't is skipped. A removal takes only what it names: the edges and children that
    /// go with a node come as removals of their own.
    pub fn apply(&mut self, ops: &[Op], cx: &mut Context<Self>) {
        self.apply_ops(ops, true, cx);
    }

    /// Applies operations to the nodes and edges, and to the stored copies too when `stored`
    /// (edits from elsewhere, which aren't sent). Without, they're an edit of this flow's, sent
    /// when it settles.
    fn apply_ops(&mut self, ops: &[Op], stored: bool, cx: &mut Context<Self>) {
        // Unsent edits of this flow's own stay unsent: the stored copies take the same ops.
        let mut at: HashMap<NodeId, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let mut edge_at: HashMap<EdgeId, usize> = self
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        let mut gone: HashSet<NodeId> = HashSet::new();
        let mut edges_gone: HashSet<EdgeId> = HashSet::new();
        let mut measured = self.shared.measured.borrow_mut();
        for op in ops {
            match op {
                Op::AddNode { node } => {
                    if !json::valid_id(&node.id) {
                        continue;
                    }
                    let mut node = (**node).clone();
                    sanitize_node(&mut node);
                    node.selected = false;
                    if stored {
                        self.stored_nodes.insert(node.id.clone(), node.clone());
                    }
                    measured.remove(&node.id);
                    match at.get(&node.id) {
                        Some(&i) => {
                            node.selected = self.nodes[i].selected && !gone.contains(&node.id);
                            gone.remove(&node.id);
                            self.nodes[i] = node;
                        }
                        None => {
                            at.insert(node.id.clone(), self.nodes.len());
                            self.nodes.push(node);
                        }
                    }
                }
                Op::PatchNode { id, patch } => {
                    if stored
                        && let Some(was) = self.stored_nodes.get_mut(id)
                        && let Some(now) = json::patched(was, patch)
                    {
                        *was = now;
                    }
                    if let Some(&i) = at.get(id)
                        && !gone.contains(id)
                        && let Some(mut now) = json::patched(&self.nodes[i], patch)
                    {
                        sanitize_node(&mut now);
                        self.nodes[i] = now;
                        measured.remove(id);
                    }
                }
                Op::RemoveNode { id } => {
                    if stored {
                        self.stored_nodes.remove(id);
                    }
                    if at.contains_key(id) {
                        gone.insert(id.clone());
                        measured.remove(id);
                    }
                }
                Op::AddEdge { edge } => {
                    if !json::valid_id(&edge.id) {
                        continue;
                    }
                    let mut edge = (**edge).clone();
                    sanitize_edge(&mut edge);
                    edge.selected = false;
                    if stored {
                        self.stored_edges.insert(edge.id.clone(), edge.clone());
                    }
                    match edge_at.get(&edge.id) {
                        Some(&i) => {
                            edge.selected =
                                self.edges[i].selected && !edges_gone.contains(&edge.id);
                            edges_gone.remove(&edge.id);
                            self.edges[i] = edge;
                        }
                        None => {
                            edge_at.insert(edge.id.clone(), self.edges.len());
                            self.edges.push(edge);
                        }
                    }
                }
                Op::PatchEdge { id, patch } => {
                    if stored
                        && let Some(was) = self.stored_edges.get_mut(id)
                        && let Some(now) = json::patched(was, patch)
                    {
                        *was = now;
                    }
                    if let Some(&i) = edge_at.get(id)
                        && !edges_gone.contains(id)
                        && let Some(mut now) = json::patched(&self.edges[i], patch)
                    {
                        sanitize_edge(&mut now);
                        self.edges[i] = now;
                    }
                }
                Op::RemoveEdge { id } => {
                    if stored {
                        self.stored_edges.remove(id);
                    }
                    if edge_at.contains_key(id) {
                        edges_gone.insert(id.clone());
                    }
                }
            }
        }
        drop(measured);
        if !gone.is_empty() {
            self.nodes.retain(|n| !gone.contains(&n.id));
            self.shared
                .ink
                .borrow_mut()
                .retain(|id, _| !gone.contains(id));
        }
        if !edges_gone.is_empty() {
            self.edges.retain(|e| !edges_gone.contains(&e.id));
        }
        self.reindex();
        self.selection_changed(cx);
        cx.notify();
    }

    /// Lets go of the measurements of nodes that are gone.
    fn forget_removed(&mut self) {
        let ids: HashSet<&NodeId> = self.nodes.iter().map(|n| &n.id).collect();
        self.shared
            .measured
            .borrow_mut()
            .retain(|id, _| ids.contains(id));
        // And the strokes read for them, which can be large.
        self.shared
            .ink
            .borrow_mut()
            .retain(|id, _| ids.contains(id));
    }

    /// Lets go of the measurements of nodes that changed since `old`, so they're drawn (and
    /// measured) again even when they're off screen.
    fn forget_changed(&mut self, old: &[Node]) {
        let old: HashMap<&NodeId, &Node> = old.iter().map(|n| (&n.id, n)).collect();
        let mut measured = self.shared.measured.borrow_mut();
        for node in &self.nodes {
            if old.get(&node.id).is_none_or(|o| !same_look(o, node)) {
                measured.remove(&node.id);
            }
        }
    }

    /// Removes the selected nodes and edges that may be deleted, with the children inside them
    /// that may be and the edges they held, and says what went. A child that may not be
    /// deleted stays where it is in the flow, no longer anyone's child. The flow's delete
    /// check can refuse. Says whether there was anything to delete.
    pub fn delete_selected(&mut self, cx: &mut Context<Self>) -> bool {
        self.edited = true;
        let deletable = |n: &Node| n.deletable.unwrap_or(true);
        let mut gone: HashSet<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.selected && deletable(n))
            .map(|n| n.id.clone())
            .collect();
        let picked: HashSet<EdgeId> = self
            .edges
            .iter()
            .filter(|e| e.selected && e.deletable.unwrap_or(true))
            .map(|e| e.id.clone())
            .collect();
        if gone.is_empty() && picked.is_empty() {
            return false;
        }
        // In a tree, a node takes what's under it, and its parent is selected after.
        let mut select_after = None;
        if self.tree_layout().is_some() {
            let tops: Vec<NodeId> = gone.iter().cloned().collect();
            select_after = tops
                .iter()
                .filter_map(|id| self.tree.parent.get(id))
                .find(|p| !gone.contains(*p))
                .cloned();
            for id in &tops {
                for d in self.tree.descendants(id) {
                    if self.node(&d).is_some_and(deletable) {
                        gone.insert(d);
                    }
                }
            }
            if select_after.as_ref().is_some_and(|p| gone.contains(p)) {
                select_after = None;
            }
        }
        loop {
            let before = gone.len();
            for node in &self.nodes {
                if deletable(node)
                    && !gone.contains(&node.id)
                    && node.parent.as_ref().is_some_and(|p| gone.contains(p))
                {
                    gone.insert(node.id.clone());
                }
            }
            if gone.len() == before {
                break;
            }
        }
        let nodes: Vec<Node> = self
            .nodes
            .iter()
            .filter(|n| gone.contains(&n.id))
            .cloned()
            .collect();
        // An edge can't outlive a node it touches.
        let edges: Vec<Edge> = self
            .edges
            .iter()
            .filter(|e| {
                picked.contains(&e.id) || gone.contains(&e.source) || gone.contains(&e.target)
            })
            .cloned()
            .collect();
        if let Some(check) = self.delete_check.clone()
            && !check(&nodes, &edges, self)
        {
            return true;
        }
        let orphans: Vec<(usize, XY)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                !gone.contains(&n.id) && n.parent.as_ref().is_some_and(|p| gone.contains(p))
            })
            .filter_map(|(i, n)| Some((i, self.absolute_position(&n.id)?)))
            .collect();
        for (i, at) in orphans {
            let n = &mut self.nodes[i];
            n.parent = None;
            n.position = at;
            if n.extent == Some(Extent::Parent) {
                n.extent = None;
            }
        }
        let cut: HashSet<EdgeId> = edges.iter().map(|e| e.id.clone()).collect();
        self.nodes.retain(|n| !gone.contains(&n.id));
        self.reindex();
        self.edges.retain(|e| !cut.contains(&e.id));
        self.forget_removed();
        cx.emit(FlowEvent::Delete { nodes, edges });
        if let Some(parent) = select_after
            && let Some(i) = self.index_of(&parent)
        {
            self.nodes[i].selected = true;
        }
        self.selection_changed(cx);
        cx.notify();
        true
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let all = self.options.elements_selectable;
        for node in &mut self.nodes {
            node.selected =
                !node.hidden && !self.folded.contains(&node.id) && node.selectable.unwrap_or(all);
        }
        for edge in &mut self.edges {
            edge.selected = !edge.hidden && edge.selectable.unwrap_or(all);
        }
        self.selection_changed(cx);
        cx.notify();
    }

    pub fn deselect_all(&mut self, cx: &mut Context<Self>) {
        self.nodes.iter_mut().for_each(|n| n.selected = false);
        self.edges.iter_mut().for_each(|e| e.selected = false);
        self.selection_changed(cx);
        cx.notify();
    }

    /// Tells subscribers when the selection isn't what they were last told.
    fn selection_changed(&mut self, cx: &mut Context<Self>) {
        let now = (self.selected_nodes(), self.selected_edges());
        if now != self.last_selection {
            self.last_selection = now.clone();
            cx.emit(FlowEvent::SelectionChange {
                nodes: now.0,
                edges: now.1,
            });
        }
    }

    /// What the canvas last said to assistive technology, in its live region: a node moved by
    /// keyboard and where to, a connection made or moved, what was deleted.
    pub fn announcement(&self) -> &str {
        &self.announcement
    }

    /// Says something to assistive technology, which reads it out without the focus moving.
    pub fn announce(&mut self, words: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.announcement = words.into();
        cx.notify();
    }

    /// A node's name by its id, or the id.
    fn name_of(&self, id: &NodeId) -> SharedString {
        self.node(id)
            .map_or_else(|| id.clone(), Node::accessible_name)
    }

    /// The node or edge the keyboard is on, if any.
    pub fn keyboard_on(&self) -> Option<&FlowItem> {
        self.keyboard_on.as_ref()
    }

    /// Puts the keyboard on a node or an edge (none: on the canvas itself), as Tab does.
    pub fn set_keyboard_on(&mut self, item: Option<FlowItem>, cx: &mut Context<Self>) {
        self.keyboard_on = item;
        self.reveal_keyboard();
        cx.notify();
    }

    /// The order Tab takes: the nodes shown, as they read (top to bottom, then left to right),
    /// then the edges shown, in their order.
    fn keyboard_order(&self) -> Vec<FlowItem> {
        let mut nodes: Vec<(XY, NodeId)> = self
            .nodes
            .iter()
            .filter(|n| self.shown(n))
            .filter_map(|n| Some((self.absolute_position(&n.id)?, n.id.clone())))
            .collect();
        nodes.sort_by(|(a, ai), (b, bi)| {
            a.y.round()
                .total_cmp(&b.y.round())
                .then(a.x.total_cmp(&b.x))
                .then(ai.cmp(bi))
        });
        let edges = self.edges.iter().filter(|e| {
            !e.hidden
                && self.node(&e.source).is_some_and(|n| self.shown(n))
                && self.node(&e.target).is_some_and(|n| self.shown(n))
        });
        nodes
            .into_iter()
            .map(|(_, id)| FlowItem::Node(id))
            .chain(edges.map(|e| FlowItem::Edge(e.id.clone())))
            .collect()
    }

    /// Tab (or shift-Tab): the keyboard to the next (or previous) node or edge. From the last
    /// (or first) it leaves them, and says so (false), for the focus to go on out of the canvas.
    fn move_keyboard(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let order = self.keyboard_order();
        let at = self
            .keyboard_on
            .as_ref()
            .and_then(|on| order.iter().position(|i| i == on));
        let next = match (at, forward) {
            (None, true) => Some(0),
            (None, false) => order.len().checked_sub(1),
            (Some(i), true) => Some(i + 1).filter(|n| *n < order.len()),
            (Some(i), false) => i.checked_sub(1),
        };
        let Some(next) = next else {
            self.keyboard_on = None;
            cx.notify();
            return false;
        };
        self.set_keyboard_on(Some(order[next].clone()), cx);
        true
    }

    /// Brings the node the keyboard is on into view, if it's out of it.
    fn reveal_keyboard(&mut self) {
        let Some(FlowItem::Node(id)) = &self.keyboard_on else {
            return;
        };
        let (Some(rect), (w, h)) = (self.node_rect(id), self.pane_size()) else {
            return;
        };
        let v = self.viewport;
        let (a, b) = (
            v.to_pane(xy(rect.x, rect.y)),
            v.to_pane(xy(rect.x + rect.w, rect.y + rect.h)),
        );
        if a.x >= 0. && a.y >= 0. && b.x <= w && b.y <= h {
            return;
        }
        let center = xy(rect.x + rect.w / 2., rect.y + rect.h / 2.);
        self.viewport = Viewport::new(
            w / 2. - center.x * v.zoom,
            h / 2. - center.y * v.zoom,
            v.zoom,
        );
    }

    /// Enter or space: selects the node or edge the keyboard is on.
    fn select_focused(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(on) = self.keyboard_on.clone() else {
            return false;
        };
        let all = self.options.elements_selectable;
        match &on {
            FlowItem::Node(id) => {
                if !self.node(id).is_some_and(|n| self.selectable(n)) {
                    return false;
                }
                self.nodes.iter_mut().for_each(|n| n.selected = &n.id == id);
                self.edges.iter_mut().for_each(|e| e.selected = false);
            }
            FlowItem::Edge(id) => {
                if !self.edge(id).is_some_and(|e| e.selectable.unwrap_or(all)) {
                    return false;
                }
                self.nodes.iter_mut().for_each(|n| n.selected = false);
                self.edges.iter_mut().for_each(|e| e.selected = &e.id == id);
            }
        }
        self.selection_changed(cx);
        cx.notify();
        true
    }

    /// Delete with nothing selected: removes the node or edge the keyboard is on, and the
    /// keyboard goes back to the canvas.
    fn delete_focused(&mut self, cx: &mut Context<Self>) -> bool {
        if self.keyboard_on.is_none() || !self.select_focused(cx) {
            return false;
        }
        self.keyboard_on = None;
        self.delete_selected(cx)
    }

    /// The ring round the node or edge the keyboard is on, which is also what assistive
    /// technology is told the keyboard is on (an active descendant of the focused canvas).
    fn focus_ring(&self, colors: &FlowColors) -> Option<AnyElement> {
        let on = self.keyboard_on.as_ref()?;
        let v = self.viewport;
        let (rect, name) = match on {
            FlowItem::Node(id) => {
                let r = self.node_rect(id)?;
                let (a, b) = (v.to_pane(xy(r.x, r.y)), v.to_pane(xy(r.x + r.w, r.y + r.h)));
                (
                    Rect::new(a.x, a.y, b.x - a.x, b.y - a.y),
                    self.node(id)?.accessible_name(),
                )
            }
            FlowItem::Edge(id) => {
                let lines = self.shared.edges.borrow();
                let line = lines.iter().find(|l| &l.id == id)?;
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for p in &line.points {
                    (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
                }
                (
                    Rect::new(x0, y0, x1 - x0, y1 - y0),
                    self.edge_name(self.edge(id)?),
                )
            }
        };
        const GAP: f32 = 4.;
        Some(
            div()
                .id("flow-keyboard-on")
                .absolute()
                .left(px(rect.x - GAP))
                .top(px(rect.y - GAP))
                .w(px(rect.w + GAP * 2.))
                .h(px(rect.h + GAP * 2.))
                .rounded(px(6.))
                .border_2()
                .border_color(colors.focus)
                .role(Role::Group)
                .aria_label(name)
                .aria_active_descendant()
                .into_any_element(),
        )
    }

    /// Says a connection was made, between which nodes.
    fn announce_connection(&mut self, edge: &Edge, cx: &mut Context<Self>) {
        let words = format!(
            "Connected {} to {}",
            self.name_of(&edge.source),
            self.name_of(&edge.target)
        );
        self.announce(words, cx);
    }

    /// What assistive technology calls an edge: its `ariaLabel` (see [`Edge::aria_label`]),
    /// else "Edge from … to …" with its nodes' names, after its label when it has one.
    pub fn edge_name(&self, edge: &Edge) -> SharedString {
        let given = edge
            .extra
            .get("ariaLabel")
            .and_then(serde_json::Value::as_str);
        if let Some(name) = given.map(str::trim).filter(|s| !s.is_empty()) {
            return name.to_string().into();
        }
        let name = |id: &NodeId| {
            self.node(id)
                .map_or_else(|| id.clone(), Node::accessible_name)
        };
        let between = format!("from {} to {}", name(&edge.source), name(&edge.target));
        match edge.label.as_ref().filter(|l| !l.trim().is_empty()) {
            Some(label) => format!("{label}, edge {between}").into(),
            None => format!("Edge {between}").into(),
        }
    }

    /// Makes the edge for a connection, as a drag between handles does, but without telling
    /// subscribers. None when a node is missing, the flow's connection check refuses it, or the
    /// edge already exists.
    pub fn connect(&mut self, connection: Connection, cx: &mut Context<Self>) -> Option<Edge> {
        self.edited = true;
        if self.node(&connection.source).is_none()
            || self.node(&connection.target).is_none()
            || !self.connection_allowed(&connection)
        {
            return None;
        }
        let d = &self.options.edge_defaults;
        let edge = Edge {
            id: json::new_id().into(),
            source_handle: connection.source_handle.clone(),
            target_handle: connection.target_handle.clone(),
            kind: d.kind.clone(),
            animated: d.animated,
            marker_start: d.marker_start,
            marker_end: d.marker_end,
            color: d.color,
            stroke_width: d.stroke_width,
            ..Edge::new("", connection.source, connection.target)
        };
        if !graph::add_edge(edge.clone(), &mut self.edges) {
            return None;
        }
        cx.notify();
        Some(edge)
    }

    fn connection_allowed(&self, connection: &Connection) -> bool {
        self.valid_connection
            .as_ref()
            .is_none_or(|check| check(connection, self))
    }

    // The viewport.

    /// The zoom limits, put right if they're not numbers above 0 or are the wrong way round.
    fn zoom_limits(&self) -> (f32, f32) {
        let ok = |z: f32, fallback: f32| if z.is_finite() && z > 0. { z } else { fallback };
        let lo = ok(self.options.min_zoom, 0.5);
        let hi = ok(self.options.max_zoom, 2.);
        (lo.min(hi), hi.max(lo))
    }

    fn clamp_zoom(&self, zoom: f32) -> f32 {
        let (lo, hi) = self.zoom_limits();
        graph::clamp(zoom, lo, hi)
    }

    /// A viewport that can be drawn: within the zoom limits, and with numbers for its parts
    /// (the current viewport's, where they aren't).
    fn sane_viewport(&self, v: Viewport) -> Viewport {
        let current = self.viewport;
        let zoom = if v.zoom.is_finite() && v.zoom > 0. {
            v.zoom
        } else if current.zoom.is_finite() && current.zoom > 0. {
            current.zoom
        } else {
            1.
        };
        let finite = |a: f32, b: f32| {
            if a.is_finite() {
                a
            } else if b.is_finite() {
                b
            } else {
                0.
            }
        };
        Viewport::new(
            finite(v.x, current.x),
            finite(v.y, current.y),
            self.clamp_zoom(zoom),
        )
    }

    /// Puts the viewport here at once.
    pub fn set_viewport(&mut self, viewport: Viewport, cx: &mut Context<Self>) {
        self.transition = None;
        self.pending_fit = None;
        self.ready = true;
        let viewport = self.sane_viewport(viewport);
        self.move_viewport(viewport, cx);
    }

    fn move_viewport(&mut self, viewport: Viewport, cx: &mut Context<Self>) {
        if viewport != self.viewport {
            self.viewport = viewport;
            cx.emit(FlowEvent::ViewportChange(viewport));
            cx.notify();
        }
    }

    /// Sends the viewport there, over the options' transition time.
    fn go_to(&mut self, to: Viewport, cx: &mut Context<Self>) {
        let to = self.sane_viewport(to);
        let duration = self.options.transition;
        if duration.is_zero() {
            self.transition = None;
            self.move_viewport(to, cx);
        } else if to != self.viewport {
            self.transition = Some(Transition {
                from: self.viewport,
                to,
                start: now(cx),
                duration,
            });
            cx.notify();
        }
    }

    /// Where the viewport is headed: the end of a transition under way, or where it is.
    fn target_viewport(&self) -> Viewport {
        self.transition.as_ref().map_or(self.viewport, |t| t.to)
    }

    /// Zooms to this level about the middle of the pane.
    pub fn zoom_to(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let (w, h) = self.pane_size();
        let to = self
            .target_viewport()
            .zoomed_about(xy(w / 2., h / 2.), self.clamp_zoom(zoom));
        self.go_to(to, cx);
    }

    /// Zooms by `factor` about this point of the window, at once, as a pinch does. GPUI doesn't
    /// recognise a two-finger pinch on iOS yet: a host that recognises one natively (a
    /// `UIPinchGestureRecognizer`) calls this with each step's change of scale, or dispatches
    /// `PinchEvent`s into the window, which the flow takes the same way.
    pub fn zoom_by(&mut self, factor: f32, about: Point<Pixels>, cx: &mut Context<Self>) {
        if !(factor.is_finite() && factor > 0.) {
            return;
        }
        self.transition = None;
        let pane = self.shared.pane_point(about);
        let zoom = self.clamp_zoom(self.viewport.zoom * factor);
        self.move_viewport(self.viewport.zoomed_about(pane, zoom), cx);
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.zoom_to(self.target_viewport().zoom * 1.2, cx);
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.zoom_to(self.target_viewport().zoom / 1.2, cx);
    }

    /// Puts this flow point in the middle of the pane, at this zoom or else the largest, as
    /// React Flow's `setCenter` does.
    pub fn set_center(&mut self, center: XY, zoom: Option<f32>, cx: &mut Context<Self>) {
        let (w, h) = self.pane_size();
        let zoom = self.clamp_zoom(zoom.unwrap_or(self.zoom_limits().1));
        self.go_to(
            Viewport::new(w / 2. - center.x * zoom, h / 2. - center.y * zoom, zoom),
            cx,
        );
    }

    /// Shows this part of the flow whole.
    pub fn fit_bounds(&mut self, bounds: Rect, padding: f32, cx: &mut Context<Self>) {
        let (w, h) = self.pane_size();
        let (lo, hi) = self.zoom_limits();
        self.go_to(
            graph::viewport_for_bounds(bounds, w, h, lo, hi, padding),
            cx,
        );
    }

    /// Shows every node. When some haven't been drawn yet, it happens once they have.
    pub fn fit_view(&mut self, cx: &mut Context<Self>) {
        self.pending_fit = Some(self.options.fit_padding);
        self.try_pending_fit(cx);
        cx.notify();
    }

    /// The pane changed size since the last frame (a window resized, a phone's keyboard up or
    /// down): a flow still as its fit left it fits again, at once; otherwise the selected node,
    /// or a tree's root, is kept in view by panning, the zoom as it was.
    fn follow_resize(&mut self, cx: &mut Context<Self>) {
        let (w, h) = self.pane_size();
        if w <= 0. || h <= 0. {
            return;
        }
        let Some(was) = self.drawn_pane.replace((w, h)) else {
            return;
        };
        if was == (w, h) || !self.ready || self.pending_fit.is_some() {
            return;
        }
        if self.fitted.is_some_and(|f| f == self.target_viewport()) {
            let Some(bounds) = self.nodes_bounds() else {
                return;
            };
            let (lo, hi) = self.zoom_limits();
            let to = self.sane_viewport(graph::viewport_for_bounds(
                bounds,
                w,
                h,
                lo,
                hi,
                self.options.fit_padding,
            ));
            self.transition = None;
            self.fitted = Some(to);
            self.move_viewport(to, cx);
            return;
        }
        let focus = self.selected_nodes().into_iter().next().or_else(|| {
            self.tree_layout()
                .and_then(|_| self.tree.roots.first().cloned())
        });
        let Some(rect) = focus.and_then(|id| self.node_rect(&id)) else {
            return;
        };
        // The part of the flow in view, and how far the node is outside it, with a margin.
        let v = self.target_viewport();
        let margin = 16. / v.zoom.max(0.01);
        let (x0, y0) = (-v.x / v.zoom, -v.y / v.zoom);
        let (x1, y1) = (x0 + w / v.zoom, y0 + h / v.zoom);
        let shift = |lo: f32, hi: f32, a: f32, b: f32| {
            if b - a > hi - lo - 2. * margin {
                // Bigger than the view: its start in view.
                lo + margin - a
            } else if a < lo + margin {
                lo + margin - a
            } else if b > hi - margin {
                hi - margin - b
            } else {
                0.
            }
        };
        let (dx, dy) = (
            shift(x0, x1, rect.x, rect.x + rect.w),
            shift(y0, y1, rect.y, rect.y + rect.h),
        );
        if dx != 0. || dy != 0. {
            self.transition = None;
            // A point at flow p is drawn at p·zoom + (x, y): moving the node by (dx, dy) on
            // screen moves the viewport by as much, scaled.
            self.move_viewport(
                self.sane_viewport(Viewport::new(v.x + dx * v.zoom, v.y + dy * v.zoom, v.zoom)),
                cx,
            );
        }
    }

    /// Does the fit that's waiting, if the pane and every node now have a size.
    fn try_pending_fit(&mut self, cx: &mut Context<Self>) {
        let Some(padding) = self.pending_fit else {
            return;
        };
        let (w, h) = self.pane_size();
        if w <= 0. || h <= 0. {
            return;
        }
        if self.nodes.iter().all(|n| !self.shown(n)) {
            self.pending_fit = None;
            self.ready = true;
            return;
        }
        let Some(bounds) = self.nodes_bounds() else {
            return;
        };
        self.pending_fit = None;
        let (lo, hi) = self.zoom_limits();
        let to = graph::viewport_for_bounds(bounds, w, h, lo, hi, padding);
        self.fitted = Some(self.sane_viewport(to));
        if self.ready {
            self.go_to(to, cx);
        } else {
            // The first frame anyone sees: no transition into it.
            self.ready = true;
            let to = self.sane_viewport(to);
            self.move_viewport(to, cx);
        }
    }

    /// Changes the pane's pattern, or takes it away.
    pub fn set_background(&mut self, background: Option<FlowBackground>, cx: &mut Context<Self>) {
        self.background = background;
        cx.notify();
    }

    /// Turns dragging, connecting and selecting on or off together, as the lock button does.
    pub fn set_interactive(&mut self, on: bool, cx: &mut Context<Self>) {
        self.options.nodes_draggable = on;
        self.options.nodes_connectable = on;
        self.options.elements_selectable = on;
        cx.notify();
    }

    pub fn interactive(&self) -> bool {
        self.options.nodes_draggable
            || self.options.nodes_connectable
            || self.options.elements_selectable
    }

    // The mouse.

    /// Whether something inside a node took the focus for the press being handled (a text
    /// field, say): the press is then its own, and the flow leaves it alone.
    fn child_took_focus(&self, window: &Window, cx: &App) -> bool {
        let Some(before) = self.shared.press_focus.borrow().clone() else {
            return false;
        };
        let now = window.focused(cx);
        now.is_some() && now != before && now.as_ref() != Some(&self.focus)
    }

    /// Whether a press on a node, a handle or a resizer is the flow's at all: not when pointer
    /// input is off, so it goes on to what's under the canvas. When it's only for moving
    /// about, the press starts a pan.
    fn press_on_content(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        match self.options.pointer_mode {
            PointerMode::Off => Some(false),
            PointerMode::Navigate => {
                if self.drag.is_none() {
                    window.focus(&self.focus, cx);
                    self.transition = None;
                    let pane = self.shared.pane_point(event.position);
                    self.start_pan(pane, event, cx);
                }
                Some(true)
            }
            PointerMode::Edit if self.tool != Tool::Select && self.drag.is_none() => {
                if event.button != MouseButton::Left {
                    return None;
                }
                window.focus(&self.focus, cx);
                self.transition = None;
                let pane = self.shared.pane_point(event.position);
                self.start_ink(pane, cx);
                Some(true)
            }
            PointerMode::Edit => None,
        }
    }

    /// What a press does while editing.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Sets what a press does while editing: select, draw with a pen, or rub out.
    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.tool != tool {
            self.tool = tool;
            cx.notify();
        }
    }

    /// Begins drawing or rubbing out at this point of the pane, as the tool says.
    fn start_ink(&mut self, pane: XY, cx: &mut Context<Self>) {
        let at = self.viewport.to_flow(pane);
        self.drag_button = MouseButton::Left;
        match self.tool {
            Tool::Pen(pen) => {
                self.drag = Some(Drag::Draw {
                    pen,
                    points: vec![crate::ink::InkPoint::at(at.x, at.y, 0., pen.width)],
                    began: now(cx),
                });
            }
            Tool::Eraser => {
                self.drag = Some(Drag::Erase {
                    last: at,
                    gone: Vec::new(),
                    edges: Vec::new(),
                });
                self.erase(at, at);
            }
            Tool::Select => {}
        }
        cx.notify();
    }

    /// Rubs out every stroke within reach of the line from `a` to `b` that may be deleted.
    fn erase(&mut self, a: XY, b: XY) {
        let reach = ERASER_REACH / self.viewport.zoom.max(0.01);
        let span = Rect::from_corners(a, b);
        let hit: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| {
                let n = &self.nodes[i];
                if n.kind != "ink" || !n.deletable.unwrap_or(true) || !self.shown(n) {
                    return false;
                }
                let (Some(r), Some(drawn), Some(abs)) = (
                    self.node_rect(&n.id),
                    self.shared.ink(n),
                    self.absolute_position(&n.id),
                ) else {
                    return false;
                };
                if r.right() < span.x - reach
                    || r.x > span.right() + reach
                    || r.bottom() < span.y - reach
                    || r.y > span.bottom() + reach
                {
                    return false;
                }
                // Along the pointer's path, a step at a time.
                let d = b - a;
                let steps = ((d.x * d.x + d.y * d.y).sqrt() / reach).ceil().max(1.) as usize;
                (0..=steps).any(|k| {
                    let t = k as f32 / steps as f32;
                    drawn.distance(xy(a.x + d.x * t, a.y + d.y * t) - abs) <= reach
                })
            })
            .collect();
        if hit.is_empty() {
            return;
        }
        self.edited = true;
        let ids: HashSet<NodeId> = hit.iter().map(|&i| self.nodes[i].id.clone()).collect();
        let mut taken = Vec::new();
        for &i in hit.iter().rev() {
            taken.push((i, self.nodes.remove(i)));
        }
        let (cut, kept): (Vec<Edge>, Vec<Edge>) = std::mem::take(&mut self.edges)
            .into_iter()
            .partition(|e| ids.contains(&e.source) || ids.contains(&e.target));
        self.edges = kept;
        self.reindex();
        self.forget_removed();
        if let Some(Drag::Erase { gone, edges, .. }) = &mut self.drag {
            gone.extend(taken);
            edges.extend(cut);
        }
    }

    /// Selects one node or adds it to the selection, and gets ready to drag what's selected.
    /// Says whether the press was the flow's.
    pub(crate) fn node_mouse_down(
        &mut self,
        id: &NodeId,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(took) = self.press_on_content(event, window, cx) {
            return took;
        }
        self.let_go_of_click_connect(cx);
        self.keyboard_on = None;
        // Inside a frame, the press is the pane's.
        let at = self.screen_to_flow(event.position);
        if self.node(id).is_some_and(|n| !self.takes_press(n, at)) {
            self.pane_mouse_down(event, window, cx);
            return true;
        }
        if self.drag.is_some() || self.child_took_focus(window, cx) {
            return true;
        }
        let Some(node) = self.node(id) else {
            return true;
        };
        let (selectable, draggable, was_selected) =
            (self.selectable(node), self.draggable(node), node.selected);
        let pane = self.shared.pane_point(event.position);
        let above = self.edge_at(pane, Some(self.node_z(node)));
        window.focus(&self.focus, cx);
        self.transition = None;
        // An edge drawn over the node takes the press.
        if let Some(edge) = above {
            self.edge_press(edge, event, cx);
            return true;
        }
        // With shift held, a press anywhere begins a selection rectangle.
        if event.modifiers.shift && self.options.elements_selectable {
            self.start_select(pane, event, cx);
            return true;
        }
        // As in React Flow: ⌘ adds or takes away; a plain press on a node that's already
        // selected keeps the whole selection, so it can all be dragged by any member.
        if selectable {
            if event.modifiers.platform {
                if let Some(i) = self.index_of(id) {
                    self.nodes[i].selected = !was_selected;
                }
            } else if !was_selected {
                self.nodes.iter_mut().for_each(|n| n.selected = &n.id == id);
                self.edges.iter_mut().for_each(|e| e.selected = false);
            }
            self.selection_changed(cx);
        }
        // What moves: the pressed node, and with it every selected node that can. A node whose
        // ancestor moves too is carried by it.
        let origins: Vec<(NodeId, XY)> = if draggable {
            self.nodes
                .iter()
                .filter(|n| {
                    (n.selected || &n.id == id) && self.draggable(n) && !self.carried(n, Some(id))
                })
                .map(|n| (n.id.clone(), n.position))
                .collect()
        } else {
            Vec::new()
        };
        self.drag_button = event.button;
        self.drag = Some(Drag::Nodes {
            start: pane,
            grab: self.viewport.to_flow(pane),
            origins,
            moved: false,
            pressed: id.clone(),
            clicks: event.click_count,
            pointer: self.viewport.to_flow(pane),
        });
        cx.notify();
        true
    }

    /// Starts dragging a connection out of a handle. Says whether the press was the flow's.
    pub(crate) fn handle_mouse_down(
        &mut self,
        node: &NodeId,
        handle: Option<SharedString>,
        kind: HandleKind,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(took) = self.press_on_content(event, window, cx) {
            return took;
        }
        if self.drag.is_some() {
            return true;
        }
        window.focus(&self.focus, cx);
        self.transition = None;
        let Some(n) = self.node(node) else {
            return true;
        };
        if !self.connectable(n) {
            return true;
        }
        let Some(at) = self.absolute_position(node) else {
            return true;
        };
        let found = {
            let measured = self.shared.measured.borrow();
            measured.get(node).and_then(|m| {
                m.handles
                    .iter()
                    .find(|h| h.kind == kind && h.id == handle)
                    .map(|h| (at + h.rect.center(), h.position))
            })
        };
        let Some((from, position)) = found else {
            return true;
        };
        let pointer = self.screen_to_flow(event.position);
        self.drag_button = event.button;
        self.drag = Some(Drag::Connect {
            from: HandleRef {
                node: node.clone(),
                handle,
                kind,
            },
            at: from,
            position,
            pointer,
            snap: None,
            started: false,
            reconnecting: None,
        });
        cx.notify();
        true
    }

    /// Starts resizing a node by one of its resizer's corners. Says whether the press was the
    /// flow's.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resize_mouse_down(
        &mut self,
        node: &NodeId,
        corner: ResizeCorner,
        min: (f32, f32),
        keep: bool,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(took) = self.press_on_content(event, window, cx) {
            return took;
        }
        if self.drag.is_some() {
            return true;
        }
        window.focus(&self.focus, cx);
        let (Some(n), Some(size)) = (self.node(node), self.node_size(node)) else {
            return true;
        };
        let drag = Drag::Resize {
            node: node.clone(),
            corner,
            start: self.screen_to_flow(event.position),
            position: n.position,
            size,
            given: (n.width, n.height),
            min,
            aspect: (keep && size.1 > 0.).then(|| size.0 / size.1),
            moved: false,
        };
        self.drag_button = event.button;
        self.drag = Some(drag);
        cx.notify();
        true
    }

    /// The topmost edge whose line passes near this point of the pane: of all of them, or of
    /// those drawn above the level `above`.
    fn edge_at(&self, pane: XY, above: Option<i64>) -> Option<EdgeId> {
        self.shared
            .edges
            .borrow()
            .iter()
            .rev()
            .filter(|line| above.is_none_or(|z| line.z > z))
            .find(|line| {
                let reach = if self.shared.touch.get() {
                    line.reach.max(self.options.touch_target_size / 2.)
                } else {
                    line.reach
                };
                distance_to_polyline(pane, &line.points) <= reach
            })
            .map(|line| line.id.clone())
    }

    /// A press on an edge: selects it (⌘ adds or takes away) and reports the click.
    fn edge_press(&mut self, id: EdgeId, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(edge) = self.edge(&id) else {
            return;
        };
        if edge.selectable.unwrap_or(self.options.elements_selectable) {
            let was = edge.selected;
            if event.modifiers.platform {
                self.edges
                    .iter_mut()
                    .filter(|e| e.id == id)
                    .for_each(|e| e.selected = !was);
            } else {
                self.edges.iter_mut().for_each(|e| e.selected = e.id == id);
                self.nodes.iter_mut().for_each(|n| n.selected = false);
            }
            self.selection_changed(cx);
        }
        let pane = self.shared.pane_point(event.position);
        cx.emit(if event.click_count >= 2 {
            FlowEvent::EdgeDoubleClick(id.clone())
        } else {
            FlowEvent::EdgeClick(id.clone())
        });
        self.start_reconnect(&id, pane, event);
        cx.notify();
    }

    /// A press near an end of an edge takes hold of that end, to move to another handle; the
    /// other end stays, and the drag is a connection from it ([`Drag::Connect`]). Says whether
    /// it did.
    fn start_reconnect(&mut self, id: &EdgeId, pane: XY, event: &MouseDownEvent) -> bool {
        if !self.options.edges_reconnectable
            || self.options.pointer_mode != PointerMode::Edit
            || event.button != MouseButton::Left
        {
            return false;
        }
        let Some((first, last)) = self
            .shared
            .edges
            .borrow()
            .iter()
            .find(|l| &l.id == id)
            .and_then(|l| Some((*l.points.first()?, *l.points.last()?)))
        else {
            return false;
        };
        let reach = if self.shared.touch.get() {
            self.options.touch_target_size / 2.
        } else {
            RECONNECT_REACH
        };
        let (to_target, to_source) = (pane.distance(last), pane.distance(first));
        let Some(edge) = self.edge(id).cloned() else {
            return false;
        };
        // The end nearer the press moves; the other is where the connection is dragged from.
        let from = if to_target <= reach && to_target <= to_source {
            HandleRef {
                node: edge.source.clone(),
                handle: edge.source_handle.clone(),
                kind: HandleKind::Source,
            }
        } else if to_source <= reach {
            HandleRef {
                node: edge.target.clone(),
                handle: edge.target_handle.clone(),
                kind: HandleKind::Target,
            }
        } else {
            return false;
        };
        let Some(at) = self.handle_point(&from.node, from.kind, from.handle.as_deref()) else {
            return false;
        };
        let position = {
            let measured = self.shared.measured.borrow();
            measured.get(&from.node).and_then(|m| {
                m.handles
                    .iter()
                    .find(|h| h.kind == from.kind && h.id == from.handle)
                    .map(|h| h.position)
            })
        };
        let Some(position) = position else {
            return false;
        };
        self.drag_button = event.button;
        self.drag = Some(Drag::Connect {
            from,
            at,
            position,
            pointer: self.viewport.to_flow(pane),
            snap: None,
            started: false,
            reconnecting: Some(id.clone()),
        });
        true
    }

    /// Moves an edge's ends to a connection's, keeping the rest of it (its id, kind, label and
    /// look), and says what it was and is. None, changing nothing, when the edge is missing,
    /// already joins those handles, another edge does, or the flow's connection check refuses
    /// it.
    pub fn reconnect(
        &mut self,
        id: &EdgeId,
        connection: Connection,
        cx: &mut Context<Self>,
    ) -> Option<(Edge, Edge)> {
        self.edited = true;
        let i = self.edges.iter().position(|e| &e.id == id)?;
        let same = |e: &Edge| {
            e.source == connection.source
                && e.target == connection.target
                && e.source_handle == connection.source_handle
                && e.target_handle == connection.target_handle
        };
        if same(&self.edges[i])
            || self.edges.iter().any(|e| &e.id != id && same(e))
            || self.node(&connection.source).is_none()
            || self.node(&connection.target).is_none()
            || !self.connection_allowed(&connection)
        {
            return None;
        }
        let old = self.edges[i].clone();
        let edge = &mut self.edges[i];
        edge.source = connection.source;
        edge.target = connection.target;
        edge.source_handle = connection.source_handle;
        edge.target_handle = connection.target_handle;
        let new = edge.clone();
        cx.notify();
        Some((old, new))
    }

    /// Begins a selection rectangle at this point of the pane; ⌘ keeps what's selected.
    fn start_select(&mut self, pane: XY, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let at = self.viewport.to_flow(pane);
        let before = (self.selected_nodes(), self.selected_edges());
        let keep = if event.modifiers.platform {
            before.0.clone()
        } else {
            Vec::new()
        };
        self.drag_button = event.button;
        self.drag = Some(Drag::Select {
            start: at,
            current: at,
            keep,
            before,
        });
        cx.notify();
    }

    // Touch.

    /// A finger touched down on part of the canvas (`target`) and offers itself. Taken, it acts
    /// as the mouse would there (a press, drags, a release), except that on the pane and edges
    /// it pans in any direction (GPUI's own touch scrolling keeps to one axis), a tap on an edge
    /// selects it, and held still it asks for a context menu. Says whether it was taken.
    pub(crate) fn touch_start(
        &mut self,
        target: &TouchTarget,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.shared.touch.set(true);
        if !self.options.touch_targets {
            // A finger, where there weren't any: they're built from the next frame.
            self.options.touch_targets = true;
            cx.notify();
        }
        let mode = self.options.pointer_mode;
        if mode == PointerMode::Off || self.drag.is_some() || self.touch.is_some() {
            return false;
        }
        let pane = self.shared.pane_point(position);
        // A node drawn as a plain box is touched through the pane.
        let target = match target {
            TouchTarget::Pane if mode == PointerMode::Edit => match self.simple_node_at(pane) {
                Some(id) => TouchTarget::Node(id),
                None => TouchTarget::Pane,
            },
            // Inside a frame, a finger is the pane's.
            TouchTarget::Node(id)
                if self
                    .node(id)
                    .is_some_and(|n| !self.takes_press(n, self.viewport.to_flow(pane))) =>
            {
                TouchTarget::Pane
            }
            other => other.clone(),
        };
        // Only moving about: the pane takes every finger, and pans.
        if mode == PointerMode::Navigate && !matches!(target, TouchTarget::Pane) {
            return false;
        }
        let node = match &target {
            TouchTarget::Pane => None,
            TouchTarget::Node(id) => Some(id.clone()),
            TouchTarget::Handle { node, .. } | TouchTarget::Resize { node, .. } => {
                Some(node.clone())
            }
        };
        if let Some(id) = &node {
            let Some(n) = self.node(id) else {
                return false;
            };
            let wants = match &target {
                TouchTarget::Node(_) => self.draggable(n) || self.selectable(n),
                TouchTarget::Handle { .. } => self.connectable(n),
                _ => true,
            };
            if !wants {
                return false;
            }
        }
        let double = self.last_tap.as_ref().is_some_and(|(at, p, on)| {
            matches!(target, TouchTarget::Pane | TouchTarget::Node(_))
                && *on == node
                && now(cx).duration_since(*at) <= DOUBLE_TAP
                && distance(*p, position) <= DOUBLE_TAP_SLOP
        });
        let down = MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: if double { 2 } else { 1 },
            first_mouse: false,
        };
        // A touch passes no capture phase for the mouse: what has the focus now is what had
        // it before the press.
        *self.shared.press_focus.borrow_mut() = Some(window.focused(cx));
        let mut edge = None;
        let took = match &target {
            TouchTarget::Pane => {
                window.focus(&self.focus, cx);
                self.transition = None;
                if mode == PointerMode::Edit {
                    edge = self.edge_at(pane, None);
                }
                self.start_pan(pane, &down, cx);
                true
            }
            TouchTarget::Node(id) => self.node_mouse_down(id, &down, window, cx),
            TouchTarget::Handle { node, handle, kind } => {
                self.handle_mouse_down(node, handle.clone(), *kind, &down, window, cx)
            }
            TouchTarget::Resize {
                node,
                corner,
                min,
                keep,
            } => self.resize_mouse_down(node, *corner, *min, *keep, &down, window, cx),
        };
        if !took || self.drag.is_none() {
            return false;
        }
        // Held still on a node, an edge or the pane, a finger asks for a context menu; on a
        // handle or a grip it's only waiting to drag.
        let hold = matches!(target, TouchTarget::Pane | TouchTarget::Node(_)).then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(LONG_PRESS).await;
                this.update(cx, |flow, cx| flow.touch_held(cx)).ok();
            })
        });
        self.touch = Some(TouchPress {
            node,
            edge,
            start: position,
            moved: false,
            hold,
            held: false,
        });
        true
    }

    /// The finger has stayed put: a context menu for what it's on, and no drag.
    fn touch_held(&mut self, cx: &mut Context<Self>) {
        let Some(press) = self.touch.as_mut() else {
            return;
        };
        press.hold = None;
        if press.moved {
            return;
        }
        press.held = true;
        let at = press.start;
        let target = match (press.node.clone(), press.edge.clone()) {
            (Some(node), _) => ContextTarget::Node(node),
            (None, Some(edge)) => ContextTarget::Edge(edge),
            (None, None) => ContextTarget::Pane,
        };
        self.cancel_drag(cx);
        self.context_menu(target, at, cx);
    }

    /// A move, lift or loss of a finger the flow took.
    fn touch_continue(&mut self, event: &TouchDragEvent, cx: &mut Context<Self>) {
        let Some(press) = self.touch.as_mut() else {
            return;
        };
        match event.phase {
            TouchPhase::Started => {}
            TouchPhase::Moved => {
                if !press.moved && distance(event.position, press.start) > TOUCH_SLOP {
                    press.moved = true;
                    press.hold = None;
                }
                // Within the slop it's still a tap, and nothing moves.
                if press.held || !press.moved {
                    return;
                }
                let moved = MouseMoveEvent {
                    position: event.position,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                };
                self.mouse_move(&moved, cx);
            }
            TouchPhase::Ended => {
                let Some(press) = self.touch.take() else {
                    return;
                };
                if press.held {
                    return;
                }
                let clicks = match &self.drag {
                    Some(Drag::Nodes { clicks, .. } | Drag::Pan { clicks, .. }) => *clicks,
                    _ => 0,
                };
                // A tap: the next one soon after, on the same thing, is a double tap.
                self.last_tap = (!press.moved && clicks == 1)
                    .then(|| (now(cx), event.position, press.node.clone()));
                if let (false, Some(edge)) = (press.moved, press.edge) {
                    self.drag = None;
                    let down = MouseDownEvent {
                        button: MouseButton::Left,
                        position: event.position,
                        modifiers: Modifiers::default(),
                        click_count: clicks.max(1),
                        first_mouse: false,
                    };
                    self.edge_press(edge, &down, cx);
                } else {
                    self.finish_drag(cx);
                }
            }
            TouchPhase::Cancelled => {
                self.touch = None;
                self.cancel_drag(cx);
            }
        }
    }

    /// Asks the host for a context menu here.
    fn context_menu(&mut self, target: ContextTarget, at: Point<Pixels>, cx: &mut Context<Self>) {
        cx.emit(FlowEvent::ContextMenu {
            target,
            position: self.screen_to_flow(at),
            window_position: at,
        });
        cx.notify();
    }

    /// A right click on a node.
    fn node_context_menu(
        &mut self,
        id: &NodeId,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.options.pointer_mode == PointerMode::Off {
            return false;
        }
        self.shared.touch.set(false);
        self.context_menu(ContextTarget::Node(id.clone()), event.position, cx);
        true
    }

    /// A right click on the pane: on an edge, or the pane itself.
    fn pane_context_menu(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if self.options.pointer_mode == PointerMode::Off || self.drag.is_some() {
            return;
        }
        self.shared.touch.set(false);
        let pane = self.shared.pane_point(event.position);
        let target = match self.edge_at(pane, None) {
            Some(edge) => ContextTarget::Edge(edge),
            None => ContextTarget::Pane,
        };
        self.context_menu(target, event.position, cx);
    }

    /// Whether a press at the flow point `at` is the node's own. A frame takes only its title
    /// and a band along its border; a press inside it is the pane's, so what's under and in it
    /// can be selected and the canvas panned from there.
    fn takes_press(&self, node: &Node, at: XY) -> bool {
        // A stroke takes a press on its ink, or a few points from it, not across its box.
        if node.kind == "ink" {
            let (Some(drawn), Some(abs)) =
                (self.shared.ink(node), self.absolute_position(&node.id))
            else {
                return true;
            };
            let reach = INK_REACH / self.viewport.zoom.max(0.01);
            return drawn.distance(at - abs) <= reach;
        }
        if !is_frame(node) {
            return true;
        }
        let Some(r) = self.node_rect(&node.id) else {
            return true;
        };
        let inner = Rect::new(
            r.x + FRAME_BORDER,
            r.y + FRAME_TITLE,
            r.w - FRAME_BORDER * 2.,
            r.h - FRAME_TITLE - FRAME_BORDER,
        );
        !(inner.w > 0. && inner.h > 0. && inner.contains(at))
    }

    /// Whether `a` is `b`, or inside it however deep.
    fn is_under(&self, a: &NodeId, b: &NodeId) -> bool {
        a == b
            || self
                .node(a)
                .is_some_and(|n| self.ancestors(n).iter().any(|p| &p.id == b))
    }

    /// Moves a node into `parent` (out of any, for none), where it is in the flow, and tells
    /// the host.
    fn set_parent(&mut self, id: &NodeId, parent: Option<NodeId>, cx: &mut Context<Self>) {
        self.edited = true;
        let (Some(at), Some(i)) = (self.absolute_position(id), self.index_of(id)) else {
            return;
        };
        if self.nodes[i].parent == parent {
            return;
        }
        let base = parent
            .as_ref()
            .and_then(|p| self.absolute_position(p))
            .unwrap_or_default();
        let node = &mut self.nodes[i];
        node.parent = parent.clone();
        node.position = at - base;
        if parent.is_none() && node.extent == Some(Extent::Parent) {
            node.extent = None;
        }
        cx.emit(FlowEvent::NodeParentChange {
            node: id.clone(),
            parent,
        });
    }

    /// After a drag in a free layout: a node dropped wholly inside a frame joins it, one
    /// dragged wholly out of its frame leaves it, and a frame dropped over loose nodes takes in
    /// those wholly inside it.
    fn adopt(&mut self, dragged: &[NodeId], cx: &mut Context<Self>) {
        self.edited = true;
        for id in dragged {
            let Some(node) = self.node(id) else {
                continue;
            };
            // Bound to its parent, it stays there.
            if node.extent == Some(Extent::Parent) {
                continue;
            }
            let Some(rect) = self.node_rect(id) else {
                continue;
            };
            // The innermost frame wholly holding it: the topmost drawn.
            let home = self
                .draw_order()
                .into_iter()
                .rev()
                .map(|(_, i)| &self.nodes[i])
                .filter(|f| is_frame(f) && !self.is_under(&f.id, id))
                .find(|f| self.node_rect(&f.id).is_some_and(|r| r.encloses(&rect)))
                .map(|f| f.id.clone());
            let current = node.parent.clone();
            let in_frame = current
                .as_ref()
                .and_then(|p| self.node(p))
                .is_some_and(is_frame);
            match (home, current) {
                (Some(frame), current) if current.as_ref() != Some(&frame) => {
                    self.set_parent(id, Some(frame), cx)
                }
                (None, Some(_)) if in_frame => self.set_parent(id, None, cx),
                _ => {}
            }
            // A frame takes in the loose nodes beside it that it now wholly covers.
            let Some(frame) = self.node(id).filter(|n| is_frame(n)) else {
                continue;
            };
            let (level, Some(area)) = (frame.parent.clone(), self.node_rect(id)) else {
                continue;
            };
            let taken: Vec<NodeId> = self
                .nodes
                .iter()
                .filter(|n| n.parent == level && &n.id != id && !self.is_under(id, &n.id))
                .filter(|n| self.shown(n))
                .filter(|n| self.node_rect(&n.id).is_some_and(|r| area.encloses(&r)))
                .map(|n| n.id.clone())
                .collect();
            for n in taken {
                self.set_parent(&n, Some(id.clone()), cx);
            }
        }
    }

    /// Puts the selected nodes in a new frame titled `title`, just large enough to hold them,
    /// and selects it. Says what the frame's id is, or none with nothing selected.
    pub fn frame_selection(
        &mut self,
        title: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> Option<NodeId> {
        self.edited = true;
        let tops: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.selected && self.shown(n))
            .filter(|n| !self.ancestors(n).iter().any(|a| a.selected))
            .map(|n| n.id.clone())
            .collect();
        let bounds = tops
            .iter()
            .filter_map(|id| self.node_rect(id))
            .reduce(|a, b| a.union(&b))?;
        // Inside the parent they share, if they share one.
        let parent = self.node(&tops[0]).and_then(|n| n.parent.clone());
        let parent = tops
            .iter()
            .all(|id| self.node(id).is_some_and(|n| n.parent == parent))
            .then_some(parent)
            .flatten();
        let pad = 24.;
        let area = Rect::new(
            bounds.x - pad,
            bounds.y - pad - FRAME_TITLE,
            bounds.w + pad * 2.,
            bounds.h + pad * 2. + FRAME_TITLE,
        );
        let base = parent
            .as_ref()
            .and_then(|p| self.absolute_position(p))
            .unwrap_or_default();
        let id: NodeId = json::new_id().into();
        let mut frame = Node::new(id.clone(), xy(area.x, area.y) - base)
            .kind("frame")
            .label(title)
            .size(area.w, area.h);
        frame.parent = parent.clone();
        self.nodes.push(frame);
        self.reindex();
        for top in &tops {
            self.set_parent(top, Some(id.clone()), cx);
        }
        for node in &mut self.nodes {
            node.selected = node.id == id;
        }
        self.edges.iter_mut().for_each(|e| e.selected = false);
        cx.emit(FlowEvent::NodeAdd {
            node: id.clone(),
            parent,
        });
        self.selection_changed(cx);
        cx.notify();
        Some(id)
    }

    /// Takes a frame away, leaving what was in it where it is, in the frame's own parent. Says
    /// whether `id` was a frame.
    pub fn unframe(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        self.edited = true;
        let Some(frame) = self.node(id).filter(|n| is_frame(n)).cloned() else {
            return false;
        };
        let inside: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.parent.as_ref() == Some(&frame.id))
            .map(|n| n.id.clone())
            .collect();
        for n in &inside {
            self.set_parent(n, frame.parent.clone(), cx);
        }
        let (gone, edges): (Vec<Edge>, Vec<Edge>) = std::mem::take(&mut self.edges)
            .into_iter()
            .partition(|e| e.source == frame.id || e.target == frame.id);
        self.edges = edges;
        self.nodes.retain(|n| n.id != frame.id);
        self.reindex();
        self.forget_removed();
        cx.emit(FlowEvent::Delete {
            nodes: vec![frame],
            edges: gone,
        });
        self.selection_changed(cx);
        cx.notify();
        true
    }

    /// Asks the host to fill in each link card that has only its address, once for each.
    fn ask_for_cards(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<(NodeId, SharedString)> = self
            .nodes
            .iter()
            .filter(|n| n.kind == "link")
            .filter_map(|n| {
                let card = n.card.as_ref()?;
                let empty = card.title.is_none() && card.description.is_none();
                (empty && !card.url.trim().is_empty()).then(|| (n.id.clone(), card.url.clone()))
            })
            .filter(|key| !self.asked.contains(key))
            .collect();
        for (node, url) in wanted {
            self.asked.insert((node.clone(), url.clone()));
            cx.emit(FlowEvent::LinkCardNeeded { node, url });
        }
    }

    // Lining up.

    /// The guides showing what the node being dragged lines up with, in the flow; empty when
    /// it lines up with nothing, or nothing's dragged.
    pub fn alignment_guides(&self) -> &[Guide] {
        &self.guides
    }

    /// Where the dragged nodes' box, moved by `by`, lines up with a node near it: `by` moved
    /// the little more it takes, and the guides to draw. Edges and middles each way line up
    /// with edges and middles, within a few points on screen.
    fn guided(&self, origins: &[(NodeId, XY)], by: XY) -> (XY, Vec<Guide>) {
        let mut area: Option<Rect> = None;
        for (id, origin) in origins {
            let (Some(node), Some((w, h))) = (self.node(id), self.node_size(id)) else {
                continue;
            };
            let parent = self
                .absolute_position(id)
                .map_or(xy(0., 0.), |at| at - node.position);
            let at = parent + *origin + by;
            let r = Rect::new(at.x, at.y, w, h);
            area = Some(area.map_or(r, |a| a.union(&r)));
        }
        let Some(area) = area else {
            return (by, Vec::new());
        };
        let reach = self.options.guide_reach / self.viewport.zoom;
        let (pw, ph) = self.pane_size();
        let seen = self.viewport.visible(pw.max(1.), ph.max(1.));
        let moving: Vec<&NodeId> = origins.iter().map(|(id, _)| id).collect();
        let others: Vec<Rect> = self
            .nodes
            .iter()
            .filter(|n| self.shown(n))
            .filter(|n| {
                !moving
                    .iter()
                    .any(|m| self.is_under(&n.id, m) || self.is_under(m, &n.id))
            })
            .filter_map(|n| self.node_rect(&n.id))
            .filter(|r| r.intersects(&seen))
            .collect();
        // The nearest line-up each way: how far, and with what.
        let best = |mine: [f32; 3], theirs: &dyn Fn(&Rect) -> [f32; 3]| {
            let mut best: Option<(f32, f32, Rect)> = None;
            for r in &others {
                for m in mine {
                    for t in theirs(r) {
                        let d = t - m;
                        if d.abs() <= reach && best.is_none_or(|(b, _, _)| d.abs() < b.abs()) {
                            best = Some((d, t, *r));
                        }
                    }
                }
            }
            best
        };
        let across = |r: &Rect| [r.x, r.x + r.w / 2., r.right()];
        let down = |r: &Rect| [r.y, r.y + r.h / 2., r.bottom()];
        let x = best(across(&area), &across);
        let y = best(down(&area), &down);
        let by = by + xy(x.map_or(0., |b| b.0), y.map_or(0., |b| b.0));
        let area = Rect::new(
            area.x + x.map_or(0., |b| b.0),
            area.y + y.map_or(0., |b| b.0),
            area.w,
            area.h,
        );
        let mut guides = Vec::new();
        if let Some((_, at, r)) = x {
            guides.push(Guide {
                from: xy(at, area.y.min(r.y)),
                to: xy(at, area.bottom().max(r.bottom())),
            });
        }
        if let Some((_, at, r)) = y {
            guides.push(Guide {
                from: xy(area.x.min(r.x), at),
                to: xy(area.right().max(r.right()), at),
            });
        }
        (by, guides)
    }

    /// The selected nodes that move as themselves (not carried by a selected parent), with
    /// their boxes in the flow.
    fn arrangeable(&self) -> Vec<(NodeId, Rect)> {
        self.nodes
            .iter()
            .filter(|n| n.selected && self.shown(n) && self.draggable(n))
            .filter(|n| !self.ancestors(n).iter().any(|a| a.selected))
            .filter_map(|n| Some((n.id.clone(), self.node_rect(&n.id)?)))
            .collect()
    }

    /// Puts a node's top left corner at `at` in the flow, wherever its parent is.
    fn place(&mut self, id: &NodeId, at: XY) {
        let (Some(abs), Some(i)) = (self.absolute_position(id), self.index_of(id)) else {
            return;
        };
        let parent = abs - self.nodes[i].position;
        let to = self.within_extent(&self.nodes[i], at - parent);
        self.nodes[i].position = to;
    }

    /// Lines the selected nodes up: their left sides with the leftmost, their middles with
    /// the selection's, and so on. Says whether there were two or more to line up.
    pub fn align_selected(&mut self, align: Align, cx: &mut Context<Self>) -> bool {
        self.edited = true;
        let boxes = self.arrangeable();
        if boxes.len() < 2 {
            return false;
        }
        let all = boxes
            .iter()
            .map(|(_, r)| *r)
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
        for (id, r) in &boxes {
            let at = match align {
                Align::Left => xy(all.x, r.y),
                Align::Center => xy(all.x + (all.w - r.w) / 2., r.y),
                Align::Right => xy(all.right() - r.w, r.y),
                Align::Top => xy(r.x, all.y),
                Align::Middle => xy(r.x, all.y + (all.h - r.h) / 2.),
                Align::Bottom => xy(r.x, all.bottom() - r.h),
            };
            self.place(id, at);
        }
        cx.emit(FlowEvent::NodeMove(
            boxes.into_iter().map(|(id, _)| id).collect(),
        ));
        cx.notify();
        true
    }

    /// Spaces the selected nodes evenly across (or down) the room the outermost ones span,
    /// keeping their order. Says whether there were three or more to space.
    pub fn distribute_selected(&mut self, axis: Distribute, cx: &mut Context<Self>) -> bool {
        self.edited = true;
        let mut boxes = self.arrangeable();
        if boxes.len() < 3 {
            return false;
        }
        let horizontal = axis == Distribute::Horizontally;
        let start = |r: &Rect| if horizontal { r.x } else { r.y };
        let length = |r: &Rect| if horizontal { r.w } else { r.h };
        boxes.sort_by(|a, b| {
            (start(&a.1) + length(&a.1) / 2.).total_cmp(&(start(&b.1) + length(&b.1) / 2.))
        });
        let first = start(&boxes[0].1);
        let last = boxes.last().map_or(first, |(_, r)| start(r) + length(r));
        let total: f32 = boxes.iter().map(|(_, r)| length(r)).sum();
        let gap = (last - first - total) / (boxes.len() - 1) as f32;
        let mut at = first;
        for (id, r) in &boxes {
            let to = if horizontal { xy(at, r.y) } else { xy(r.x, at) };
            self.place(id, to);
            at += length(r) + gap;
        }
        cx.emit(FlowEvent::NodeMove(
            boxes.into_iter().map(|(id, _)| id).collect(),
        ));
        cx.notify();
        true
    }

    /// Whether nodes are drawn as plain boxes at the current zoom.
    fn simplified(&self) -> bool {
        self.viewport.zoom < self.options.simplify_below_zoom
    }

    /// The topmost node drawn as a plain box under this point of the pane.
    fn simple_node_at(&self, pane: XY) -> Option<NodeId> {
        if !self.simplified() {
            return None;
        }
        let at = self.viewport.to_flow(pane);
        self.draw_order()
            .into_iter()
            .rev()
            .map(|(_, i)| &self.nodes[i])
            .find(|n| {
                self.node_rect(&n.id).is_some_and(|r| r.contains(at)) && self.takes_press(n, at)
            })
            .map(|n| n.id.clone())
    }

    /// A press that no node, handle or panel took: on a node drawn as a box it's that node's,
    /// on an edge it selects the edge, and otherwise it starts a selection rectangle or a pan.
    fn pane_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self.options.pointer_mode;
        if self.drag.is_some() || mode == PointerMode::Off {
            return;
        }
        self.let_go_of_click_connect(cx);
        self.keyboard_on = None;
        let pane = self.shared.pane_point(event.position);
        if mode == PointerMode::Edit
            && event.button == MouseButton::Left
            && self.tool != Tool::Select
        {
            window.focus(&self.focus, cx);
            self.transition = None;
            self.start_ink(pane, cx);
            return;
        }
        if mode == PointerMode::Edit
            && event.button == MouseButton::Left
            && let Some(id) = self.simple_node_at(pane)
        {
            self.node_mouse_down(&id, event, window, cx);
            return;
        }
        window.focus(&self.focus, cx);
        self.transition = None;
        if mode == PointerMode::Edit && event.button == MouseButton::Left {
            if let Some(id) = self.edge_at(pane, None) {
                self.edge_press(id, event, cx);
                return;
            }
            if self.options.elements_selectable
                && (event.modifiers.shift || self.options.selection_on_drag)
            {
                self.start_select(pane, event, cx);
                return;
            }
        }
        self.start_pan(pane, event, cx);
    }

    /// Begins panning from this point of the pane.
    fn start_pan(&mut self, pane: XY, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.drag_button = event.button;
        self.drag = Some(Drag::Pan {
            start: pane,
            last: pane,
            from: self.viewport,
            moved: false,
            clicks: if event.button == MouseButton::Left {
                event.click_count
            } else {
                0
            },
        });
        cx.notify();
    }

    fn minimap_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if self.drag.is_some() {
            return;
        }
        self.transition = None;
        self.drag_button = event.button;
        self.drag = Some(Drag::Minimap);
        self.minimap_center(event.position, cx);
    }

    /// Centers the pane on the flow point under this point of the minimap.
    fn minimap_center(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(map) = self.shared.minimap.get() else {
            return;
        };
        let at = xy(
            map.origin.x + f32::from(position.x - map.bounds.origin.x) / map.scale,
            map.origin.y + f32::from(position.y - map.bounds.origin.y) / map.scale,
        );
        let (w, h) = self.pane_size();
        let zoom = self.viewport.zoom;
        let to = self.sane_viewport(Viewport::new(
            w / 2. - at.x * zoom,
            h / 2. - at.y * zoom,
            zoom,
        ));
        self.move_viewport(to, cx);
    }

    /// A point on the grid, when snapping is on. An axis whose spacing isn't a number above 0
    /// doesn't snap.
    fn snapped(&self, p: XY) -> XY {
        if !self.options.snap_to_grid {
            return p;
        }
        let (gx, gy) = self.options.snap_grid;
        let snap = |v: f32, g: f32| {
            if g.is_finite() && g > 0. {
                (v / g).round() * g
            } else {
                v
            }
        };
        xy(snap(p.x, gx), snap(p.y, gy))
    }

    /// How far the dragged nodes move for a pointer that's moved `by`. With snapping, the
    /// pressed node's corner lands on the grid of the flow (not its parent's), and the others
    /// move as far, keeping their places around it.
    fn drag_offset(&self, origins: &[(NodeId, XY)], pressed: &NodeId, by: XY) -> XY {
        if !self.options.snap_to_grid {
            return by;
        }
        let Some((id, origin)) = origins
            .iter()
            .find(|(id, _)| id == pressed)
            .or(origins.first())
        else {
            return by;
        };
        let Some(node) = self.node(id) else {
            return by;
        };
        let parent = self
            .absolute_position(id)
            .map_or(xy(0., 0.), |at| at - node.position);
        let start = parent + *origin;
        self.snapped(start + by) - start
    }

    /// Where a node may be put, nearest `p` (inside its parent): within its parent's box or its
    /// own area of the flow, when it has an extent.
    fn within_extent(&self, node: &Node, p: XY) -> XY {
        let Some(extent) = node.extent else {
            return p;
        };
        let own = self.node_size(&node.id).unwrap_or((0., 0.));
        match extent {
            Extent::Parent => {
                let Some(parent) = node.parent.as_ref().and_then(|id| self.node_size(id)) else {
                    return p;
                };
                xy(
                    graph::clamp(p.x, 0., (parent.0 - own.0).max(0.)),
                    graph::clamp(p.y, 0., (parent.1 - own.1).max(0.)),
                )
            }
            Extent::Area(area) => {
                let offset = self
                    .absolute_position(&node.id)
                    .map_or(xy(0., 0.), |at| at - node.position);
                let at = p + offset;
                xy(
                    graph::clamp(at.x, area.x, (area.right() - own.0).max(area.x)),
                    graph::clamp(at.y, area.y, (area.bottom() - own.1).max(area.y)),
                ) - offset
            }
        }
    }

    /// Puts every node with an extent inside it, as drawing would show it: a child saved
    /// outside its parent, or left outside when the parent shrank.
    fn enforce_extents(&mut self) {
        for i in 0..self.nodes.len() {
            if self.nodes[i].extent.is_none() {
                continue;
            }
            let p = self.nodes[i].position;
            let to = self.within_extent(&self.nodes[i], p);
            if to != p {
                self.nodes[i].position = to;
                self.edited = true;
            }
        }
    }

    /// The handle a dragged connection's end is over or nearest, within reach, and whether it
    /// may take the connection. As in React Flow: a handle under the pointer first, then the
    /// nearest within the connection radius (in flow units), and of equally near ones, one of
    /// the other kind.
    fn snap_for(&self, from: &HandleRef, pointer: XY) -> Option<Snap> {
        let reach = self.options.connection_radius.max(0.);
        let loose = self.options.connection_mode == ConnectionMode::Loose;
        // Handles sit on a node's edge and stick out a little; a node farther than this can't
        // have one in reach.
        let margin = reach + 16.;
        let mut best: Option<((f32, bool), Snap)> = None;
        let measured = self.shared.measured.borrow();
        for node in self.nodes.iter().filter(|n| self.shown(n)) {
            let Some(m) = measured.get(&node.id) else {
                continue;
            };
            let Some(at) = self.absolute_position(&node.id) else {
                continue;
            };
            if let Some((w, h)) = m.size {
                let near = Rect::new(
                    at.x - margin,
                    at.y - margin,
                    w + margin * 2.,
                    h + margin * 2.,
                );
                if !near.contains(pointer) {
                    continue;
                }
            }
            for h in &m.handles {
                if node.id == from.node && h.kind == from.kind && h.id == from.handle {
                    continue;
                }
                let rect = Rect::new(at.x + h.rect.x, at.y + h.rect.y, h.rect.w, h.rect.h);
                let center = rect.center();
                let d = if rect.contains(pointer) {
                    0.
                } else {
                    center.distance(pointer)
                };
                if d > reach {
                    continue;
                }
                let same_kind = h.kind == from.kind;
                if best
                    .as_ref()
                    .is_some_and(|((bd, bs), _)| *bd < d || (*bd == d && (*bs <= same_kind)))
                {
                    continue;
                }
                // The edge runs from the source handle to the target handle, whichever end the
                // drag started from.
                let connection = if from.kind == HandleKind::Source {
                    Connection {
                        source: from.node.clone(),
                        source_handle: from.handle.clone(),
                        target: node.id.clone(),
                        target_handle: h.id.clone(),
                    }
                } else {
                    Connection {
                        source: node.id.clone(),
                        source_handle: h.id.clone(),
                        target: from.node.clone(),
                        target_handle: from.handle.clone(),
                    }
                };
                let kinds_fit = loose || !same_kind;
                // Loose, a handle can't be joined to another of the same id on its own node.
                let same_handle = loose && node.id == from.node && h.id == from.handle;
                let valid = kinds_fit
                    && !same_handle
                    && h.connectable
                    && self.connectable(node)
                    && self.connection_allowed(&connection);
                best = Some((
                    (d, same_kind),
                    Snap {
                        connection,
                        to: HandleRef {
                            node: node.id.clone(),
                            handle: h.id.clone(),
                            kind: h.kind,
                        },
                        point: center,
                        position: h.position,
                        valid,
                    },
                ));
            }
        }
        best.map(|(_, snap)| snap)
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        // The button came up somewhere the window didn't see.
        if event.pressed_button.is_none() {
            self.drag = Some(drag);
            self.finish_drag(cx);
            return;
        }
        let pane = self.shared.pane_point(event.position);
        let pointer = self.viewport.to_flow(pane);
        self.drag_pane = Some(pane);
        match &mut drag {
            Drag::Draw { pen, points, began } => {
                let t = now(cx).saturating_duration_since(*began).as_secs_f32();
                if points.len() < crate::ink::MAX_POINTS {
                    points.push(crate::ink::InkPoint::at(pointer.x, pointer.y, t, pen.width));
                }
                self.drag = Some(drag);
                cx.notify();
                return;
            }
            Drag::Erase { last, .. } => {
                let from = std::mem::replace(last, pointer);
                self.drag = Some(drag);
                self.erase(from, pointer);
                cx.notify();
                return;
            }
            Drag::Minimap => {
                self.drag = Some(drag);
                self.minimap_center(event.position, cx);
                return;
            }
            _ => {}
        }
        self.drag = Some(drag);
        self.drag_to(pane, cx);
    }

    /// The drag under way follows the pointer at `pane` (in the pane): the pointer moved, or
    /// the viewport moved under it.
    fn drag_to(&mut self, pane: XY, cx: &mut Context<Self>) {
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let pointer = self.viewport.to_flow(pane);
        match &mut drag {
            // Taken by mouse_move.
            Drag::Draw { .. } | Drag::Erase { .. } | Drag::Minimap => {}
            Drag::Pan {
                start, last, moved, ..
            } => {
                if !*moved && (pane - *start).distance(xy(0., 0.)) < 2. {
                    self.drag = Some(drag);
                    return;
                }
                *moved = true;
                let delta = pane - *last;
                *last = pane;
                if self.options.pan_on_drag {
                    let v = self.viewport;
                    self.move_viewport(Viewport::new(v.x + delta.x, v.y + delta.y, v.zoom), cx);
                }
            }
            Drag::Nodes {
                start,
                grab,
                origins,
                moved,
                pressed,
                pointer: at,
                ..
            } => {
                self.edited = true;
                *at = pointer;
                if !*moved
                    && (pane - *start).distance(xy(0., 0.)) < self.options.node_drag_threshold
                {
                    self.drag = Some(drag);
                    return;
                }
                if !origins.is_empty() {
                    let ids: Vec<NodeId> = origins.iter().map(|(id, _)| id.clone()).collect();
                    if !*moved {
                        cx.emit(FlowEvent::NodeDragStart(ids.clone()));
                    }
                    let by = self.drag_offset(origins, pressed, pointer - *grab);
                    let by = if self.options.alignment_guides && self.tree_layout().is_none() {
                        let (by, guides) = self.guided(origins, by);
                        self.guides = guides;
                        by
                    } else {
                        by
                    };
                    for (id, origin) in origins.iter() {
                        let Some(i) = self.index_of(id) else {
                            continue;
                        };
                        let to = self.within_extent(&self.nodes[i], *origin + by);
                        self.nodes[i].position = to;
                    }
                    cx.emit(FlowEvent::NodeDrag(ids));
                }
                *moved = true;
                cx.notify();
            }
            Drag::Select {
                start,
                current,
                keep,
                ..
            } => {
                *current = pointer;
                let area = Rect::from_corners(*start, pointer);
                let partial = self.options.selection_mode == SelectionMode::Partial;
                let picked: HashSet<NodeId> = self
                    .nodes
                    .iter()
                    .filter(|n| self.shown(n) && self.selectable(n))
                    .filter(|n| {
                        keep.contains(&n.id)
                            || self.node_rect(&n.id).is_some_and(|r| {
                                if partial {
                                    area.intersects(&r)
                                } else {
                                    area.encloses(&r)
                                }
                            })
                    })
                    .map(|n| n.id.clone())
                    .collect();
                for node in &mut self.nodes {
                    node.selected = picked.contains(&node.id);
                }
                // As in React Flow: an edge is taken with either of its nodes.
                let all = self.options.elements_selectable;
                for edge in &mut self.edges {
                    edge.selected = edge.selectable.unwrap_or(all)
                        && !edge.hidden
                        && (picked.contains(&edge.source) || picked.contains(&edge.target));
                }
                self.selection_changed(cx);
                cx.notify();
            }
            Drag::Connect {
                from,
                pointer: at,
                snap,
                started,
                reconnecting,
                ..
            } => {
                if !*started {
                    *started = true;
                    cx.emit(match reconnecting {
                        Some(edge) => FlowEvent::ReconnectStart(edge.clone()),
                        None => FlowEvent::ConnectStart {
                            node: from.node.clone(),
                            handle: from.handle.clone(),
                            kind: from.kind,
                        },
                    });
                }
                *at = pointer;
                *snap = self.snap_for(from, pointer);
                cx.notify();
            }
            Drag::Resize {
                node,
                corner,
                start,
                position,
                size,
                min,
                aspect,
                moved,
                ..
            } => {
                self.edited = true;
                if !*moved {
                    *moved = true;
                    cx.emit(FlowEvent::NodeResizeStart(node.clone()));
                }
                let delta = pointer - *start;
                let left = matches!(corner, ResizeCorner::TopLeft | ResizeCorner::BottomLeft);
                let top = matches!(corner, ResizeCorner::TopLeft | ResizeCorner::TopRight);
                let mut w = (size.0 + if left { -delta.x } else { delta.x }).max(min.0);
                let mut h = (size.1 + if top { -delta.y } else { delta.y }).max(min.1);
                // Keeping proportions, the way the pointer went further decides the size.
                if let Some(a) = *aspect {
                    if w / size.0 >= h / size.1 {
                        h = w / a;
                    } else {
                        w = h * a;
                    }
                    if w < min.0 || h < min.1 {
                        w = min.0.max(min.1 * a);
                        h = w / a;
                    }
                }
                // The corner opposite the one held stays where it is.
                let x = if left {
                    position.x + size.0 - w
                } else {
                    position.x
                };
                let y = if top {
                    position.y + size.1 - h
                } else {
                    position.y
                };
                if let Some(i) = self.index_of(node) {
                    let n = &mut self.nodes[i];
                    n.width = Some(w);
                    n.height = Some(h);
                    n.position = xy(x, y);
                }
                cx.emit(FlowEvent::NodeResize(node.clone()));
                cx.notify();
            }
        }
        self.drag = Some(drag);
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        // Another button coming up doesn't end the drag.
        if self.drag.is_some() && event.button == self.drag_button {
            self.finish_drag(cx);
        }
    }

    /// Ends whatever the mouse was doing, as its release does.
    fn finish_drag(&mut self, cx: &mut Context<Self>) {
        self.guides.clear();
        self.drag_pane = None;
        self.auto_panned = None;
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag {
            Drag::Draw { pen, points, .. } => {
                // Smoothed and thinned to what shows at the zoom it was drawn at.
                let tolerance = 0.35 / self.viewport.zoom.max(0.01);
                let points = crate::ink::smooth(&points, tolerance);
                let stroke =
                    crate::ink::InkStroke::anchored(pen.tool, pen.color, pen.width as f64, points);
                self.edited = true;
                self.nodes.push(Node::ink(json::new_id(), &stroke));
                self.reindex();
                cx.notify();
            }
            Drag::Erase { .. } => {
                self.edited = true;
                cx.notify();
            }
            Drag::Pan {
                start,
                moved,
                clicks,
                ..
            } => {
                if !moved && clicks >= 1 {
                    let at = self.viewport.to_flow(start);
                    if clicks >= 2 && self.options.zoom_on_double_click {
                        let zoom = self.clamp_zoom(self.viewport.zoom * 2.);
                        self.go_to(self.viewport.zoomed_about(start, zoom), cx);
                    } else if self.options.pointer_mode == PointerMode::Edit {
                        self.deselect_all(cx);
                        cx.emit(FlowEvent::PaneClick(at));
                    }
                }
            }
            Drag::Nodes {
                origins,
                moved,
                pressed,
                clicks,
                pointer,
                ..
            } => {
                if moved {
                    if !origins.is_empty() {
                        let ids: Vec<NodeId> = origins.into_iter().map(|(id, _)| id).collect();
                        // In a tree, dropped on another node, they become its children; else
                        // the layout takes them back where they were.
                        if let Some(parent) = self.drop_target(&ids, pointer) {
                            for id in &ids {
                                self.reparent(id, &parent, cx);
                            }
                        } else if self.tree_layout().is_none() {
                            self.adopt(&ids, cx);
                        }
                        cx.emit(FlowEvent::NodeDragStop(ids));
                    }
                } else {
                    cx.emit(if clicks >= 2 {
                        FlowEvent::NodeDoubleClick(pressed)
                    } else {
                        FlowEvent::NodeClick(pressed)
                    });
                }
            }
            Drag::Connect {
                reconnecting: Some(edge),
                snap,
                started,
                ..
            } => {
                if started {
                    let moved = snap
                        .filter(|s| s.valid)
                        .and_then(|s| self.reconnect(&edge, s.connection, cx));
                    let reconnected = moved.is_some();
                    if let Some((old, new)) = moved {
                        let words = format!("Reconnected: {}", self.edge_name(&new));
                        self.announce(words, cx);
                        cx.emit(FlowEvent::Reconnect { old, new });
                    }
                    cx.emit(FlowEvent::ReconnectEnd { edge, reconnected });
                }
            }
            Drag::Connect {
                from,
                snap,
                started,
                pointer,
                at,
                ..
            } => {
                if !started && self.options.connect_on_click {
                    self.click_connect(from, at, cx);
                } else if started {
                    self.shared.click_connect.borrow_mut().take();
                    let to = snap.as_ref().map(|s| s.to.clone());
                    let made = snap
                        .filter(|s| s.valid)
                        .and_then(|s| self.connect(s.connection, cx));
                    let connected = made.is_some();
                    if let Some(edge) = made {
                        self.announce_connection(&edge, cx);
                        cx.emit(FlowEvent::Connect(edge));
                    }
                    cx.emit(FlowEvent::ConnectEnd {
                        from,
                        to,
                        connected,
                        pointer,
                    });
                }
            }
            Drag::Resize { node, moved, .. } => {
                if moved {
                    cx.emit(FlowEvent::NodeResizeEnd(node));
                }
            }
            Drag::Select { .. } | Drag::Minimap => {}
        }
        cx.notify();
    }

    fn scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        if self.options.pointer_mode == PointerMode::Off {
            return;
        }
        cx.stop_propagation();
        self.transition = None;
        let pane = self.shared.pane_point(event.position);
        let (delta, lines) = match event.delta {
            ScrollDelta::Pixels(p) => (xy(f32::from(p.x), f32::from(p.y)), false),
            ScrollDelta::Lines(l) => (xy(l.x, l.y), true),
        };
        let zoom_key = event.modifiers.platform || event.modifiers.control;
        if event.source == ScrollSource::Touch {
            // A finger dragged over the pane: the flow follows it exactly.
            self.shared.touch.set(true);
            let v = self.viewport;
            let to = self.sane_viewport(Viewport::new(v.x + delta.x, v.y + delta.y, v.zoom));
            self.move_viewport(to, cx);
        } else if self.options.pan_on_scroll && !zoom_key {
            // React Flow counts a line as 20 points.
            let k = self.options.pan_on_scroll_speed * if lines { 20. } else { 1. };
            let v = self.viewport;
            let to =
                self.sane_viewport(Viewport::new(v.x + delta.x * k, v.y + delta.y * k, v.zoom));
            self.move_viewport(to, cx);
        } else if self.options.zoom_on_scroll || (self.options.pan_on_scroll && zoom_key) {
            // React Flow's wheel: 2^(0.002) a point, 2^(0.05) a line.
            let k = if lines { 0.05 } else { 0.002 };
            let zoom = self.clamp_zoom(self.viewport.zoom * 2f32.powf(delta.y * k));
            self.move_viewport(self.viewport.zoomed_about(pane, zoom), cx);
        }
    }

    fn pinch(&mut self, event: &PinchEvent, cx: &mut Context<Self>) {
        if !self.options.zoom_on_pinch || self.options.pointer_mode == PointerMode::Off {
            return;
        }
        cx.stop_propagation();
        self.zoom_by(1. + event.delta, event.position, cx);
    }

    /// Moves the selected nodes a step (four with `far`), as the arrow keys do: five points, or
    /// a grid square when snapping. Says whether anything moved.
    fn nudge(&mut self, dx: f32, dy: f32, far: bool, cx: &mut Context<Self>) -> bool {
        self.edited = true;
        let snap = self.options.snap_to_grid;
        let (sx, sy) = if snap {
            self.options.snap_grid
        } else {
            (5., 5.)
        };
        let k = if far { 4. } else { 1. };
        let by = xy(dx * sx * k, dy * sy * k);
        let moving: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.selected && self.draggable(n) && !self.carried(n, None))
            .map(|n| n.id.clone())
            .collect();
        // With nothing selected, the node the keyboard is on.
        let moving = if moving.is_empty() {
            match &self.keyboard_on {
                Some(FlowItem::Node(id)) if self.node(id).is_some_and(|n| self.draggable(n)) => {
                    vec![id.clone()]
                }
                _ => moving,
            }
        } else {
            moving
        };
        if moving.is_empty() {
            return false;
        }
        for id in &moving {
            let Some(i) = self.index_of(id) else {
                continue;
            };
            let node = &self.nodes[i];
            let mut to = node.position + by;
            if snap {
                let parent = self
                    .absolute_position(id)
                    .map_or(xy(0., 0.), |at| at - node.position);
                to = self.snapped(parent + to) - parent;
            }
            let to = self.within_extent(node, to);
            self.nodes[i].position = to;
        }
        // Where to, as React Flow says it.
        let words = match moving.as_slice() {
            [one] => {
                let at = self.node(one).map_or(xy(0., 0.), |n| n.position);
                format!(
                    "Moved {}. New position, x: {}, y: {}",
                    self.name_of(one),
                    at.x.round(),
                    at.y.round()
                )
            }
            many => format!("Moved {} nodes", many.len()),
        };
        self.announce(words, cx);
        cx.emit(FlowEvent::NodeMove(moving));
        cx.notify();
        true
    }

    /// Escape: drops what's being dragged, putting back what it changed, or else the
    /// selection. Says whether there was anything to do.
    fn cancel(&mut self, cx: &mut Context<Self>) -> bool {
        if self.cancel_drag(cx) {
            return true;
        }
        if self.let_go_of_click_connect(cx) {
            return true;
        }
        if self.nodes.iter().all(|n| !n.selected) && self.edges.iter().all(|e| !e.selected) {
            // Then the keyboard leaves the node or edge it was on, for the canvas; with
            // neither, escape goes on to the host.
            if self.keyboard_on.take().is_some() {
                cx.notify();
                return true;
            }
            return false;
        }
        self.deselect_all(cx);
        cx.notify();
        true
    }

    /// A handle clicked rather than dragged from ([`FlowOptions::connect_on_click`]): it
    /// begins a connection, or finishes the one a click on another handle began, as a drag
    /// from that one dropped here would; the same handle again lets it go.
    fn click_connect(&mut self, clicked: HandleRef, at: XY, cx: &mut Context<Self>) {
        let first = self.shared.click_connect.borrow_mut().take();
        match first {
            None => {
                cx.emit(FlowEvent::ConnectStart {
                    node: clicked.node.clone(),
                    handle: clicked.handle.clone(),
                    kind: clicked.kind,
                });
                *self.shared.click_connect.borrow_mut() = Some(clicked);
            }
            Some(first) if first == clicked => cx.emit(FlowEvent::ConnectEnd {
                from: first,
                to: None,
                connected: false,
                pointer: at,
            }),
            Some(first) => {
                let snap = self.snap_for(&first, at).filter(|s| s.to == clicked);
                let made = snap
                    .filter(|s| s.valid)
                    .and_then(|s| self.connect(s.connection, cx));
                let connected = made.is_some();
                if let Some(edge) = made {
                    self.announce_connection(&edge, cx);
                    cx.emit(FlowEvent::Connect(edge));
                }
                cx.emit(FlowEvent::ConnectEnd {
                    from: first,
                    to: Some(clicked),
                    connected,
                    pointer: at,
                });
            }
        }
        cx.notify();
    }

    /// Lets go of a connection a click on a handle began, as a press elsewhere or escape
    /// does. Says whether there was one.
    fn let_go_of_click_connect(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(from) = self.shared.click_connect.borrow_mut().take() else {
            return false;
        };
        let pointer = self
            .handle_point(&from.node, from.kind, from.handle.as_deref())
            .unwrap_or_default();
        cx.emit(FlowEvent::ConnectEnd {
            from,
            to: None,
            connected: false,
            pointer,
        });
        cx.notify();
        true
    }

    /// Drops what's being dragged, putting back what it changed. Says whether anything was.
    fn cancel_drag(&mut self, cx: &mut Context<Self>) -> bool {
        self.edited = true;
        self.guides.clear();
        match self.drag.take() {
            Some(Drag::Draw { .. }) => {}
            Some(Drag::Erase { gone, edges, .. }) => {
                // Back where they were, in the order they came out.
                for (i, node) in gone.into_iter().rev() {
                    let i = i.min(self.nodes.len());
                    self.nodes.insert(i, node);
                }
                self.edges.extend(edges);
                self.reindex();
            }
            Some(Drag::Nodes { origins, moved, .. }) => {
                for (id, origin) in &origins {
                    if let Some(i) = self.index_of(id) {
                        self.nodes[i].position = *origin;
                    }
                }
                if moved && !origins.is_empty() {
                    cx.emit(FlowEvent::NodeDragStop(
                        origins.into_iter().map(|(id, _)| id).collect(),
                    ));
                }
            }
            Some(Drag::Resize {
                node,
                position,
                given,
                moved,
                ..
            }) => {
                if let Some(i) = self.index_of(&node) {
                    let n = &mut self.nodes[i];
                    n.position = position;
                    (n.width, n.height) = given;
                }
                if moved {
                    cx.emit(FlowEvent::NodeResizeEnd(node));
                }
            }
            Some(Drag::Pan { from, moved, .. }) => {
                if moved {
                    self.move_viewport(from, cx);
                }
            }
            Some(Drag::Select { before, .. }) => {
                for node in &mut self.nodes {
                    node.selected = before.0.contains(&node.id);
                }
                for edge in &mut self.edges {
                    edge.selected = before.1.contains(&edge.id);
                }
                self.selection_changed(cx);
            }
            Some(Drag::Connect {
                reconnecting: Some(edge),
                started,
                ..
            }) => {
                if started {
                    cx.emit(FlowEvent::ReconnectEnd {
                        edge,
                        reconnected: false,
                    });
                }
            }
            Some(Drag::Connect {
                from,
                started,
                pointer,
                ..
            }) => {
                if started {
                    cx.emit(FlowEvent::ConnectEnd {
                        from,
                        to: None,
                        connected: false,
                        pointer,
                    });
                }
            }
            Some(Drag::Minimap) => {}
            None => return false,
        }
        cx.notify();
        true
    }

    // Drawing.

    /// While nodes or a connection are dragged with the pointer near the pane's edge, moves the
    /// viewport that way a little each frame, the drag following the pointer as it does, as
    /// React Flow's auto-pan. Its speed is React Flow's per frame at 60 a second, scaled to the
    /// time since the last frame, so a 120-hertz display moves no faster.
    fn auto_pan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on = match &self.drag {
            Some(Drag::Nodes {
                moved: true,
                origins,
                ..
            }) => self.options.auto_pan_on_node_drag && !origins.is_empty(),
            Some(Drag::Connect { started: true, .. }) => self.options.auto_pan_on_connect,
            _ => false,
        };
        let Some(pane) = self.drag_pane.filter(|_| on) else {
            self.auto_panned = None;
            return;
        };
        let (w, h) = self.pane_size();
        let (dx, dy) = auto_pan_velocity(pane, (w, h), self.options.auto_pan_speed);
        if dx == 0. && dy == 0. {
            self.auto_panned = None;
            return;
        }
        let now = self.frame_time;
        // The first frame near the edge starts the clock; later ones move by the time since.
        if let Some(last) = self.auto_panned {
            let frames = now.saturating_duration_since(last).as_secs_f32().min(0.1) * 60.;
            if frames > 0. {
                let v = self.viewport;
                self.move_viewport(
                    Viewport::new(v.x + dx * frames, v.y + dy * frames, v.zoom),
                    cx,
                );
                self.drag_to(pane, cx);
            }
        }
        self.auto_panned = Some(now);
        window.request_animation_frame();
    }

    /// Moves a transition along, and asks for another frame until it's done.
    fn step_transition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.transition else {
            return;
        };
        let elapsed = now(cx).duration_since(t.start);
        let p = (elapsed.as_secs_f32() / t.duration.as_secs_f32()).min(1.);
        // Ease out: fast away, gentle in.
        let e = 1. - (1. - p).powi(3);
        let lerp = |a: f32, b: f32| a + (b - a) * e;
        let v = Viewport::new(
            lerp(t.from.x, t.to.x),
            lerp(t.from.y, t.to.y),
            lerp(t.from.zoom, t.to.zoom),
        );
        if p >= 1. {
            let to = t.to;
            self.transition = None;
            self.move_viewport(to, cx);
        } else {
            if v != self.viewport {
                self.viewport = v;
                cx.emit(FlowEvent::ViewportChange(v));
            }
            window.request_animation_frame();
        }
    }

    /// Animated edges move by redrawing twenty times a second, and only while one is on screen.
    fn keep_ticking(&mut self, cx: &mut Context<Self>) {
        if self.ticker.is_some() || !self.shared.animating.get() {
            return;
        }
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let go_on = this.update(cx, |flow, cx| {
                    if flow.shared.animating.get() {
                        cx.notify();
                        true
                    } else {
                        flow.ticker = None;
                        false
                    }
                });
                if !go_on.unwrap_or(false) {
                    break;
                }
            }
        }));
    }

    /// The nodes to draw, bottom first, with the level each is drawn at: parents under their
    /// children, and what's selected (and what's inside it) above the rest. Of two nodes with
    /// one id only the first is drawn, and a node off screen only once it's been measured.
    fn draw_order(&self) -> Vec<(i64, usize)> {
        self.draw_order_in(&self.geo())
    }

    /// The part of the flow in view, and a margin round it.
    fn visible_area(&self) -> Option<Rect> {
        let (mut w, mut h) = self.pane_size();
        if w <= 0. {
            // Before the pane's first layout: the window's size, which it's no larger than.
            (w, h) = self.shared.window.get();
            if w <= 0. {
                return None;
            }
        }
        let margin = OVERDRAW / self.viewport.zoom;
        let v = self.viewport.visible(w, h);
        Some(Rect::new(
            v.x - margin,
            v.y - margin,
            v.w + margin * 2.,
            v.h + margin * 2.,
        ))
    }

    /// What to draw, bottom first: what's in view, and of what's never been drawn (so has no
    /// size yet) a few hundred more a frame until all of it has been, so a large flow opens
    /// over a few frames rather than in one long one. A fit waiting for every size draws all
    /// of it at once. Says, in `self.shared.unmeasured`, how many are still to come.
    fn draw_order_in(&self, geo: &Geo) -> Vec<(i64, usize)> {
        let cull = self.options.only_render_visible;
        let visible = self.visible_area();
        let mut budget = if self.pending_fit.is_some() {
            usize::MAX
        } else {
            MEASURE_PER_FRAME
        };
        let mut left = 0;
        let mut order: Vec<(i64, usize)> = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if !geo.first[i] || !self.shown(n) {
                continue;
            }
            let draw = match (cull, visible) {
                (true, Some(v)) if geo.measured[i] => geo.rect[i].is_none_or(|r| r.intersects(&v)),
                (true, Some(v)) => {
                    let at = geo.at[i];
                    let guess =
                        Rect::new(at.x, at.y, n.width.unwrap_or(1.), n.height.unwrap_or(1.));
                    if guess.intersects(&v) {
                        true
                    } else if budget > 0 {
                        budget -= 1;
                        true
                    } else {
                        left += 1;
                        false
                    }
                }
                _ => true,
            };
            if draw {
                order.push((geo.z[i], i));
            }
        }
        self.shared.unmeasured.set(left);
        order.sort_unstable();
        order
    }

    fn render_node(
        &self,
        node: &Node,
        colors: FlowColors,
        this: &WeakEntity<Flow>,
        window: &mut Window,
        cx: &mut App,
    ) -> Zoomed {
        let zoom = self.viewport.zoom;
        let origin = self.absolute_position(&node.id).unwrap_or(node.position);
        let at = self.viewport.to_pane(origin);
        let ctx = NodeContext {
            flow: this.clone(),
            shared: self.shared.clone(),
            id: node.id.clone(),
            origin,
            selected: node.selected,
            connectable: self.connectable(node),
            zoom,
            colors,
            touch_size: px(self.options.touch_target_size.max(0.)),
            touch_targets: self.options.touch_targets,
            depth: self.tree_layout().map(|_| self.tree.depth(&node.id)),
            children: self.tree.children_of(&node.id).len(),
            collapsed: node.collapsed,
            link: node.link.clone(),
            images: self.image_loader.clone(),
        };
        let renderer = self
            .node_kinds
            .get(&node.kind)
            .or_else(|| self.node_kinds.get("default"))
            .cloned();
        let content = renderer.map(|render| render(node, &ctx, self, window, cx));
        let dragging = matches!(&self.drag, Some(Drag::Nodes { moved: true, .. }));
        let editing = self.options.pointer_mode == PointerMode::Edit;
        let id = node.id.clone();
        let flow = this.clone();
        let wrapper = div()
            .absolute()
            .left(px(at.x))
            .top(px(at.y))
            .text_size(ctx.font_size(12.))
            .text_color(colors.node_text)
            .when_some(node.width, |d, w| d.w(units(w)))
            .when_some(node.height, |d, h| d.h(units(h)))
            .when(!self.ready, |d| d.opacity(0.))
            .when(
                editing && self.tool == Tool::Select && self.draggable(node),
                |d| {
                    d.cursor(if dragging {
                        CursorStyle::ClosedHand
                    } else {
                        CursorStyle::OpenHand
                    })
                },
            )
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let took = flow
                    .update(cx, |flow, cx| flow.node_mouse_down(&id, event, window, cx))
                    .unwrap_or(false);
                if took {
                    cx.stop_propagation();
                }
            })
            .on_mouse_down(MouseButton::Right, {
                let (flow, id) = (this.clone(), node.id.clone());
                move |event, _, cx| {
                    let took = flow
                        .update(cx, |flow, cx| flow.node_context_menu(&id, event, cx))
                        .unwrap_or(false);
                    if took {
                        cx.stop_propagation();
                    }
                }
            })
            .when(self.options.touch_targets, |d| {
                d.child(touch_target(
                    this.clone(),
                    TouchTarget::Node(node.id.clone()),
                    px(0.),
                    Reach::Around,
                ))
            })
            .when_some(self.collapse_chip(node, &ctx, this), |d, chip| {
                d.child(chip)
            })
            .child(measure_node(
                self.shared.clone(),
                this.entity_id(),
                node.id.clone(),
            ))
            .children(content);
        // A group named for the node, for assistive technology (React Flow's node wrapper).
        let name = node.accessible_name();
        gpui_core::audit("FlowNode", Role::Group, Some(&name));
        let wrapper = wrapper
            .id(SharedString::from(format!("node-{}", node.id)))
            .role(Role::Group)
            .aria_label(name);
        Zoomed::new(zoom, wrapper)
    }

    /// In a tree, the round chip on the side a node's children leave from that folds them away
    /// or shows them again: a minus, or how many are folded.
    fn collapse_chip(
        &self,
        node: &Node,
        ctx: &NodeContext,
        this: &WeakEntity<Flow>,
    ) -> Option<Stateful<Div>> {
        if ctx.children == 0 || self.tree_layout().is_none() {
            return None;
        }
        let side = self.placed.get(&node.id).map_or(Side::Right, |p| p.out);
        let size = units(16.);
        // Centred 10 units past the side.
        let out = units(-18.);
        let label: SharedString = if node.collapsed {
            self.tree.descendants(&node.id).len().to_string().into()
        } else {
            "−".into()
        };
        let base = div()
            .id(SharedString::from(format!("collapse-{}", node.id)))
            .absolute()
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(ctx.colors.node_border)
            .bg(ctx.colors.panel)
            .text_size(ctx.font_size(9.))
            .text_color(ctx.colors.muted_text)
            .cursor(CursorStyle::PointingHand)
            .nodrag()
            .child(label);
        let placed = match side {
            Side::Right => base.right(out).top(relative(0.5)).mt(units(-8.)),
            Side::Left => base.left(out).top(relative(0.5)).mt(units(-8.)),
            Side::Bottom => base.bottom(out).left(relative(0.5)).ml(units(-8.)),
            Side::Top => base.top(out).left(relative(0.5)).ml(units(-8.)),
        };
        let (flow, id, collapsed) = (this.clone(), node.id.clone(), node.collapsed);
        Some(placed.on_click(move |_, _, cx| {
            flow.update(cx, |flow, cx| flow.set_collapsed(&id, !collapsed, cx))
                .ok();
        }))
    }

    /// What the edge canvases need of each edge, so they don't hold the flow while they paint.
    fn edge_draws(&self, colors: &FlowColors, geo: &Geo) -> Vec<EdgeDraw> {
        let visible = self.visible_area();
        // What routed edges go round: every node drawn but frames and groups, which edges run
        // inside of.
        let obstacles: Option<Rc<Vec<Rect>>> = self.options.avoid_nodes.then(|| {
            Rc::new(
                self.nodes
                    .iter()
                    .filter(|n| self.shown(n) && !is_frame(n) && n.kind != "group")
                    .filter_map(|n| self.node_rect(&n.id))
                    .collect(),
            )
        });
        // The edge whose end is being moved is drawn as the connection line instead.
        let moving = match &self.drag {
            Some(Drag::Connect {
                reconnecting: Some(edge),
                started: true,
                ..
            }) => Some(edge.clone()),
            _ => None,
        };
        self.edges
            .iter()
            .filter(|e| !e.hidden && moving.as_ref() != Some(&e.id))
            .filter_map(|e| {
                let (si, ti) = (self.index_of(&e.source)?, self.index_of(&e.target)?);
                let (source, target) = (&self.nodes[si], &self.nodes[ti]);
                if !self.shown(source) || !self.shown(target) {
                    return None;
                }
                // Off screen, and far enough that no curve, route or label of it can reach in.
                if self.options.only_render_visible
                    && let (Some(v), Some(a), Some(b)) = (visible, geo.rect[si], geo.rect[ti])
                {
                    let span = a.union(&b);
                    let pad = span.w.max(span.h) * 0.25 + OVERDRAW / self.viewport.zoom;
                    let reach = Rect::new(
                        span.x - pad,
                        span.y - pad,
                        span.w + pad * 2.,
                        span.h + pad * 2.,
                    );
                    if !reach.intersects(&v) {
                        return None;
                    }
                }
                Some(EdgeDraw {
                    id: e.id.clone(),
                    z: self.edge_z(e, [(source, geo.z[si]), (target, geo.z[ti])]),
                    kind: e.kind.clone(),
                    source: End {
                        node: source.id.clone(),
                        at: geo.at[si],
                        handle: e.source_handle.clone(),
                    },
                    target: End {
                        node: target.id.clone(),
                        at: geo.at[ti],
                        handle: e.target_handle.clone(),
                    },
                    label: e.label.clone(),
                    animated: e.animated,
                    marker_start: e.marker_start,
                    marker_end: e.marker_end,
                    color: if e.selected {
                        colors.selected
                    } else {
                        e.color.map(|c| rgb(c).into()).unwrap_or_else(|| {
                            e.pencil
                                .as_deref()
                                .and_then(|k| colors.palette.get(k))
                                .unwrap_or(colors.edge)
                        })
                    },
                    dash: e.dash,
                    label_color: match e.pencil.as_deref() {
                        Some(k) if colors.palette.get(k).is_some() => colors.pencil(Some(k)).text,
                        _ => colors.edge_label,
                    },
                    obstacles: matches!(e.kind, EdgeKind::Step | EdgeKind::SmoothStep)
                        .then(|| obstacles.clone())
                        .flatten(),
                    width: e.stroke_width.unwrap_or(1.) + if e.selected { 0.5 } else { 0. },
                    stroke: e.stroke_width.unwrap_or(1.),
                    reach: e.interaction_width.unwrap_or(INTERACTION_WIDTH) / 2.,
                    anchors: self.tree_anchors(e),
                })
            })
            .collect()
    }

    /// A canvas that paints these edges, all of one level. It paints after every node has
    /// said where its handles are this frame.
    fn edge_layer(
        &self,
        z: i64,
        draws: Vec<EdgeDraw>,
        colors: FlowColors,
        flow_id: EntityId,
    ) -> AnyElement {
        let shared = self.shared.clone();
        let viewport = self.viewport;
        let zoom = viewport.zoom;
        let loose = self.options.connection_mode == ConnectionMode::Loose;
        let phase = self.frame_time.duration_since(self.started).as_secs_f32() * DASH_SPEED * zoom;
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let origin = bounds.origin;
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let clip = Rect::new(-OVERDRAW, -OVERDRAW, w + OVERDRAW * 2., h + OVERDRAW * 2.);
                let pad = OVERDRAW / zoom;
                let seen = viewport.visible(w, h);
                let seen = Rect::new(
                    seen.x - pad,
                    seen.y - pad,
                    seen.w + pad * 2.,
                    seen.h + pad * 2.,
                );
                let measured = shared.measured.borrow();
                let mut lines = Vec::with_capacity(draws.len());
                let mut ink = paint::Ink::clipped(origin, clip);
                let mut labels = Vec::new();
                let mut animated = false;
                for edge in &draws {
                    let (s, t) = (&edge.source, &edge.target);
                    let ends = edge.anchors.or_else(|| {
                        endpoint(
                            &measured,
                            &s.node,
                            s.at,
                            &s.handle,
                            HandleKind::Source,
                            loose,
                        )
                        .zip(endpoint(
                            &measured,
                            &t.node,
                            t.at,
                            &t.handle,
                            HandleKind::Target,
                            loose,
                        ))
                    });
                    let Some(((from, from_side), (to, to_side))) = ends else {
                        continue;
                    };
                    let path = match &edge.obstacles {
                        Some(obstacles) => {
                            routed_path(&shared, edge, from, from_side, to, to_side, obstacles)
                                .unwrap_or_else(|| {
                                    edge_path(&edge.kind, from, from_side, to, to_side)
                                })
                        }
                        None => edge_path(&edge.kind, from, from_side, to, to_side),
                    };
                    // Off screen, an edge costs nothing more.
                    if !path.bounds().intersects(&seen) && !seen.contains(from) {
                        continue;
                    }
                    let line: Vec<XY> = path
                        .flatten(3. / zoom)
                        .into_iter()
                        .map(|p| viewport.to_pane(p))
                        .collect();
                    let width = (edge.width * zoom).max(0.75);
                    if edge.animated {
                        ink.dashed(&line, width, DASH * zoom, DASH * zoom, phase, edge.color);
                        animated = true;
                    } else {
                        match edge.dash {
                            EdgeDash::Solid => ink.line(&line, width, edge.color),
                            EdgeDash::Dashed => {
                                ink.dashed(&line, width, 6. * zoom, 4. * zoom, 0., edge.color)
                            }
                            EdgeDash::Dotted => ink.dashed(
                                &line,
                                width,
                                width.max(1.5 * zoom),
                                4. * zoom,
                                0.,
                                edge.color,
                            ),
                        }
                    }
                    let n = line.len();
                    if n >= 2 {
                        if let Some(marker) = &edge.marker_end {
                            ink.marker(
                                marker,
                                line[n - 1],
                                line[n - 2],
                                zoom * edge.stroke.max(1.),
                                width,
                                edge.color,
                            );
                        }
                        if let Some(marker) = &edge.marker_start {
                            ink.marker(
                                marker,
                                line[0],
                                line[1],
                                zoom * edge.stroke.max(1.),
                                width,
                                edge.color,
                            );
                        }
                    }
                    if let Some(label) = &edge.label {
                        labels.push((label, viewport.to_pane(path.label), edge.label_color));
                    }
                    lines.push(EdgeLine {
                        id: edge.id.clone(),
                        z,
                        reach: (edge.reach * zoom).max(MIN_REACH),
                        points: line,
                        label: edge.label.as_ref().map(|_| viewport.to_pane(path.label)),
                    });
                }
                // Every line first, as a path or two, and then the labels over them.
                ink.paint(window);
                for (label, at, color) in labels {
                    paint::paint_label(origin, label, at, zoom, &colors, color, window, cx);
                }
                drop(measured);
                shared.edges.borrow_mut().extend(lines);
                if animated {
                    shared.animating.set(true);
                    // The first frame with something animated on screen: the flow starts its
                    // ticker when it next renders.
                    if !shared.was_animating.replace(true) {
                        cx.notify(flow_id);
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }

    fn control_button(
        &self,
        icon: Icon,
        colors: FlowColors,
        cx: &mut Context<Self>,
        act: fn(&mut Flow, &mut Context<Flow>),
    ) -> Div {
        div()
            .size(px(26.))
            .cursor(CursorStyle::PointingHand)
            .hover(move |s| s.bg(colors.panel_hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |flow, _, _, cx| {
                    cx.stop_propagation();
                    act(flow, cx);
                }),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        paint::paint_icon(icon, bounds, colors.panel_icon, window)
                    },
                )
                .size_full(),
            )
    }

    fn render_controls(&self, colors: FlowColors, cx: &mut Context<Self>) -> Div {
        let lock = if self.interactive() {
            Icon::Unlocked
        } else {
            Icon::Locked
        };
        let line = || div().h(px(1.)).w_full().bg(colors.panel_border);
        div()
            .absolute()
            .left(px(15.))
            .bottom(px(15.))
            .flex()
            .flex_col()
            .rounded(px(6.))
            .overflow_hidden()
            .border_1()
            .border_color(colors.panel_border)
            .bg(colors.panel)
            .occlude()
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .child(self.control_button(Icon::Plus, colors, cx, |f, cx| f.zoom_in(cx)))
            .child(line())
            .child(self.control_button(Icon::Minus, colors, cx, |f, cx| f.zoom_out(cx)))
            .child(line())
            .child(self.control_button(Icon::Fit, colors, cx, |f, cx| f.fit_view(cx)))
            .child(line())
            .child(self.control_button(lock, colors, cx, |f, cx| {
                f.set_interactive(!f.interactive(), cx)
            }))
    }

    fn render_minimap(&self, colors: FlowColors, cx: &mut Context<Self>) -> Div {
        let nodes: Vec<(Rect, bool)> = self
            .nodes
            .iter()
            .filter(|n| self.shown(n))
            .filter_map(|n| Some((self.node_rect(&n.id)?, n.selected)))
            .collect();
        let (w, h) = self.pane_size();
        let visible = self.viewport.visible(w.max(1.), h.max(1.));
        let shared = self.shared.clone();
        div()
            .absolute()
            .right(px(15.))
            .bottom(px(15.))
            .w(px(200.))
            .h(px(150.))
            .rounded(px(6.))
            .overflow_hidden()
            .border_1()
            .border_color(colors.panel_border)
            .bg(colors.panel)
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|flow, event, _, cx| {
                    cx.stop_propagation();
                    flow.minimap_mouse_down(event, cx);
                }),
            )
            .on_scroll(cx.listener(|flow, event: &ScrollEvent, _, cx| {
                // Scrolling over the minimap zooms about the middle of the pane.
                cx.stop_propagation();
                let dy = match event.delta {
                    ScrollDelta::Pixels(p) => f32::from(p.y) * 0.002,
                    ScrollDelta::Lines(l) => l.y * 0.05,
                };
                let (w, h) = flow.pane_size();
                let zoom = flow.clamp_zoom(flow.viewport.zoom * 2f32.powf(dy));
                flow.transition = None;
                flow.move_viewport(flow.viewport.zoomed_about(xy(w / 2., h / 2.), zoom), cx);
            }))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        paint::paint_minimap(bounds, &nodes, visible, &colors, &shared, window)
                    },
                )
                .size_full(),
            )
    }
}

/// One end of an edge, as the edge canvas needs it.
struct End {
    node: NodeId,
    at: XY,
    handle: Option<SharedString>,
}

struct EdgeDraw {
    id: EdgeId,
    z: i64,
    kind: EdgeKind,
    source: End,
    target: End,
    label: Option<SharedString>,
    animated: bool,
    marker_start: Option<Marker>,
    marker_end: Option<Marker>,
    color: Hsla,
    width: f32,
    /// The edge's own stroke width, in flow units, unselected: what its arrowheads grow with.
    stroke: f32,
    /// Half the interaction width, in flow units.
    reach: f32,
    /// Where a tree edge meets its nodes, set by the layout rather than by handles.
    anchors: Option<((XY, Side), (XY, Side))>,
    dash: EdgeDash,
    label_color: Hsla,
    /// The nodes a routed edge goes round, in the flow; none for an edge that isn't routed.
    obstacles: Option<Rc<Vec<Rect>>>,
}

/// A node drawn as a plain box, in pane coordinates.
struct NodeBox {
    rect: Rect,
    fill: Hsla,
    border: Hsla,
    look: BoxLook,
}

/// What a node too small to read still shows of itself: a whiteboard's shapes, colors and ink
/// stay as they are, so a board zoomed right out still looks like itself.
enum BoxLook {
    Plain,
    /// The outline, `w` flow units wide.
    Shape(crate::shapes::NodeShape, f32),
    /// A stroke, drawn as it is, in its color.
    Ink(Rc<crate::ink::Drawn>, Hsla, f32),
    /// A heading with no box: a bar where its text is.
    Text,
}

/// A canvas that paints nodes too small to read as boxes of their size and colors.
fn box_layer(boxes: Vec<NodeBox>) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let o = bounds.origin;
            for b in &boxes {
                let area = Bounds::new(
                    point(o.x + px(b.rect.x), o.y + px(b.rect.y)),
                    size(px(b.rect.w), px(b.rect.h)),
                );
                match &b.look {
                    BoxLook::Plain => window.paint_quad(quad(
                        area,
                        px(1.),
                        b.fill,
                        px(1.),
                        b.border,
                        BorderStyle::default(),
                    )),
                    // Too small for its outline to show (and outlines cost a tessellation
                    // each): a box, round for the round shapes.
                    BoxLook::Shape(shape, _) if b.rect.w.min(b.rect.h) < 24. => {
                        use crate::shapes::NodeShape::*;
                        let r = match shape {
                            Ellipse | Cloud => b.rect.w.min(b.rect.h) / 2.,
                            Rounded | Hexagon | Diamond => b.rect.w.min(b.rect.h) / 4.,
                            _ => 1.,
                        };
                        window.paint_quad(quad(
                            area,
                            px(r),
                            b.fill,
                            px(1.),
                            b.border,
                            BorderStyle::default(),
                        ));
                    }
                    BoxLook::Shape(shape, w) => {
                        let h = *w * b.rect.h / b.rect.w.max(0.01);
                        let g = crate::shapes::geometry(*shape, *w, h);
                        paint::paint_shape(&g.outline, &[], area, *w, b.fill, b.border, 1., window);
                    }
                    BoxLook::Ink(drawn, color, zoom) => {
                        let halo = (b.border != b.fill).then_some(b.border);
                        paint::paint_ink(&drawn.outline, area.origin, *zoom, *color, halo, window);
                    }
                    BoxLook::Text => {
                        let bar = Bounds::new(
                            point(area.origin.x, area.origin.y + area.size.height * 0.3),
                            size(area.size.width, area.size.height * 0.4),
                        );
                        window.paint_quad(quad(
                            bar,
                            px(1.),
                            b.border,
                            px(0.),
                            b.border,
                            BorderStyle::default(),
                        ));
                    }
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

/// Where an edge meets a node, and which way it leaves, as React Flow finds it: the handle of
/// the end's kind with the edge's handle id, or the first of that kind when it names none.
/// Loose, a handle of the other kind will do. None when there's no such handle: the edge isn't
/// drawn.
fn endpoint(
    measured: &FastMap<NodeId, Measured>,
    node: &NodeId,
    at: XY,
    handle: &Option<SharedString>,
    kind: HandleKind,
    loose: bool,
) -> Option<(XY, Side)> {
    let m = measured.get(node)?;
    let handle = handle.as_ref().filter(|h| !h.is_empty());
    let named = |h: &&crate::node::HandleBounds| handle.is_none_or(|id| h.id.as_ref() == Some(id));
    let found = m
        .handles
        .iter()
        .filter(|h| h.kind == kind)
        .find(named)
        .or_else(|| {
            if loose {
                m.handles.iter().find(named)
            } else {
                None
            }
        })?;
    Some((at + found.anchor(), found.position))
}

impl Render for Flow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors.unwrap_or_else(|| FlowColors::from_theme(cx));
        let window_size = window.viewport_size();
        self.shared
            .window
            .set((f32::from(window_size.width), f32::from(window_size.height)));
        self.enforce_extents();
        // The keyboard can't stay on something gone or hidden.
        let gone = match &self.keyboard_on {
            Some(FlowItem::Node(id)) => !self.node(id).is_some_and(|n| self.shown(n)),
            Some(FlowItem::Edge(id)) => !self.edge(id).is_some_and(|e| !e.hidden),
            None => false,
        };
        if gone {
            self.keyboard_on = None;
        }
        self.frame_time = now(cx);
        self.ask_for_cards(cx);
        self.lay_out_tree(self.frame_time, window);
        self.follow_resize(cx);
        self.try_pending_fit(cx);
        self.step_transition(window, cx);
        self.auto_pan(window, cx);
        self.keep_ticking(cx);

        let viewport = self.viewport;
        let zoom = viewport.zoom;
        let shared = self.shared.clone();
        shared.viewport.set(viewport);
        let this = cx.entity().downgrade();
        let flow_id = cx.entity_id();
        let ready = self.ready;

        // Under everything: the pane's own bounds, its pattern, and the listeners that follow
        // the mouse wherever a drag takes it.
        let pane = {
            let shared = shared.clone();
            let background = self.background;
            let (move_flow, up_flow) = (this.clone(), this.clone());
            let (touch_flow, press_flow) = (this.clone(), this.clone());
            let listener = self.transform_listener.clone();
            canvas(
                move |bounds, window, cx| {
                    if shared.pane.replace(bounds).size != bounds.size {
                        // A pending fit and what's on screen depend on the pane's size.
                        cx.notify(flow_id);
                        window.request_animation_frame();
                    }
                    // In the frame it changes, so a native view kept in line moves with it.
                    let transform = CanvasTransform {
                        pane: bounds,
                        viewport,
                    };
                    if let Some(listener) = &listener
                        && shared.transform.replace(Some(transform)) != Some(transform)
                    {
                        listener(transform, window, cx);
                    }
                    let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                    (shared, hitbox)
                },
                move |bounds, (shared, hitbox), window, _| {
                    // The edge canvases, painted after this, say again what they drew.
                    shared.edges.borrow_mut().clear();
                    shared.was_animating.set(shared.animating.replace(false));
                    if let Some(background) = &background {
                        paint::paint_background(background, bounds, viewport, &colors, window);
                    }
                    // Before anything handles a press: what had the focus, so a press that a
                    // node's child takes the focus for is left to it.
                    let focus_shared = shared.clone();
                    window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            *focus_shared.press_focus.borrow_mut() = Some(window.focused(cx));
                        }
                    });
                    let touch_shared = shared.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            // The mouse moved, so presses are the mouse's again.
                            touch_shared.touch.set(false);
                            move_flow
                                .update(cx, |flow, cx| flow.mouse_move(event, cx))
                                .ok();
                        }
                    });
                    // A finger that took hold of part of a node, wherever it goes.
                    let touch_flow = touch_flow.clone();
                    window.on_mouse_event(move |event: &TouchDragEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble && event.phase != TouchPhase::Started {
                            touch_flow
                                .update(cx, |flow, cx| flow.touch_continue(event, cx))
                                .ok();
                        }
                    });
                    // A finger on the pane, or on a node that didn't take it: the pane's.
                    let (press_flow, hitbox) = (press_flow.clone(), hitbox.clone());
                    window.on_mouse_event(move |event: &TouchDragEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.phase == TouchPhase::Started
                            && !window.default_prevented()
                            && hitbox.is_hovered(window)
                        {
                            let claimed = press_flow
                                .update(cx, |flow, cx| {
                                    flow.touch_start(
                                        &TouchTarget::Pane,
                                        event.start_position,
                                        window,
                                        cx,
                                    )
                                })
                                .unwrap_or(false);
                            if claimed {
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }
                    });
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            up_flow.update(cx, |flow, cx| flow.mouse_up(event, cx)).ok();
                        }
                    });
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };

        // The nodes, bottom first, with the edges of each level drawn just under the nodes of
        // that level: an edge between two children sits above their parent.
        let geo = self.geo();
        let mut draws = if ready {
            self.edge_draws(&colors, &geo)
        } else {
            Vec::new()
        };
        // Routes of edges that are gone are let go of.
        if self.options.avoid_nodes && self.shared.routes.borrow().len() > self.edges.len() {
            let ids: HashSet<&EdgeId> = self.edges.iter().map(|e| &e.id).collect();
            self.shared
                .routes
                .borrow_mut()
                .retain(|id, _| ids.contains(id));
        }
        draws.sort_by_key(|d| d.z);
        let mut levels: Vec<(i64, Vec<EdgeDraw>)> = Vec::new();
        for draw in draws {
            match levels.last_mut() {
                Some((z, list)) if *z == draw.z => list.push(draw),
                _ => levels.push((draw.z, vec![draw])),
            }
        }
        let mut levels = levels.into_iter().peekable();
        let simple = self.simplified();
        let mut layers: Vec<AnyElement> = Vec::new();
        let mut boxes: Vec<NodeBox> = Vec::new();
        let order = self.draw_order_in(&geo);
        if self.shared.unmeasured.get() > 0 {
            // More to measure: the next frame takes the next few hundred.
            window.request_animation_frame();
        }
        for (z, i) in order {
            while let Some((level, _)) = levels.peek()
                && *level <= z
            {
                let (level, list) = levels.next().unwrap_or_default();
                if !boxes.is_empty() {
                    layers.push(box_layer(std::mem::take(&mut boxes)));
                }
                layers.push(self.edge_layer(level, list, colors, flow_id));
            }
            let node = &self.nodes[i];
            let measured = self
                .shared
                .measured
                .borrow()
                .get(&node.id)
                .is_some_and(|m| m.size.is_some());
            // Too small to read: a box of the node's size, which it measured when last drawn.
            if simple
                && measured
                && let Some(rect) = self.node_rect(&node.id)
            {
                let a = viewport.to_pane(xy(rect.x, rect.y));
                let pencil = colors.pencil(node.color.as_deref());
                let (mut fill, mut border) = match node.kind.as_ref() {
                    "group" | "frame" => (colors.group, colors.node_border),
                    _ => (pencil.fill, pencil.stroke),
                };
                let look = match node.kind.as_ref() {
                    "shape" => BoxLook::Shape(node.shape.unwrap_or_default(), rect.w),
                    "ink" => match self.shared.ink(node) {
                        Some(drawn) => {
                            let color = paint::ink_color(drawn.color, drawn.tool, &colors);
                            fill = color;
                            border = color;
                            BoxLook::Ink(drawn, color, zoom)
                        }
                        None => BoxLook::Plain,
                    },
                    "text" => {
                        border = Hsla {
                            a: 0.35,
                            ..pencil.text
                        };
                        BoxLook::Text
                    }
                    _ => BoxLook::Plain,
                };
                if node.selected {
                    border = colors.selected;
                }
                boxes.push(NodeBox {
                    rect: Rect::new(a.x, a.y, rect.w * zoom, rect.h * zoom),
                    fill,
                    border,
                    look,
                });
                continue;
            }
            if !boxes.is_empty() {
                layers.push(box_layer(std::mem::take(&mut boxes)));
            }
            layers.push(
                self.render_node(node, colors, &this, window, cx)
                    .into_any_element(),
            );
        }
        if !boxes.is_empty() {
            layers.push(box_layer(boxes));
        }
        for (level, list) in levels {
            layers.push(self.edge_layer(level, list, colors, flow_id));
        }

        // Over the nodes: the connection being dragged, the selection rectangle, and in a tree
        // the node a drag would be dropped on.
        let drop_onto = match &self.drag {
            Some(Drag::Nodes {
                moved: true,
                origins,
                pointer,
                ..
            }) => {
                let ids: Vec<NodeId> = origins.iter().map(|(id, _)| id.clone()).collect();
                self.drop_target(&ids, *pointer)
                    .and_then(|id| self.node_rect(&id))
            }
            _ => None,
        };
        let guides = self.guides.clone();
        let overlay = {
            let drag = self.drag.clone();
            let line_kind = self.options.connection_line.clone();
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let origin = bounds.origin;
                    for g in &guides {
                        let line = [viewport.to_pane(g.from), viewport.to_pane(g.to)];
                        paint::stroke(origin, &line, 1., colors.guide, window);
                    }
                    if let Some(r) = drop_onto {
                        let a = viewport.to_pane(xy(r.x, r.y));
                        let pad = 4.;
                        let area = Bounds::new(
                            point(origin.x + px(a.x - pad), origin.y + px(a.y - pad)),
                            size(px(r.w * zoom + pad * 2.), px(r.h * zoom + pad * 2.)),
                        );
                        window.paint_quad(quad(
                            area,
                            px(8. * zoom),
                            colors.selection_fill,
                            px(2.),
                            colors.selected,
                            BorderStyle::default(),
                        ));
                    }
                    match &drag {
                        Some(Drag::Connect {
                            at,
                            position,
                            pointer,
                            snap,
                            started: true,
                            ..
                        }) => {
                            // As in React Flow: the line ends on a handle that would take it,
                            // and otherwise follows the pointer, red over one that wouldn't.
                            let (to, to_side, color) = match snap {
                                Some(s) if s.valid => (s.point, s.position, colors.selected),
                                Some(_) => (*pointer, position.opposite(), colors.invalid),
                                None => (*pointer, position.opposite(), colors.edge),
                            };
                            let line: Vec<XY> = edge_path(&line_kind, *at, *position, to, to_side)
                                .flatten(3. / zoom)
                                .into_iter()
                                .map(|p| viewport.to_pane(p))
                                .collect();
                            paint::stroke(origin, &line, zoom.max(0.75), color, window);
                        }
                        Some(Drag::Draw { pen, points, .. }) => {
                            let stroke = crate::ink::InkStroke::new(
                                pen.tool,
                                pen.color,
                                pen.width as f64,
                                points.clone(),
                            );
                            let color = paint::ink_color(pen.color, pen.tool, &colors);
                            let outline: Vec<(XY, f32)> = stroke
                                .outline()
                                .into_iter()
                                // In pane points over the zoom: the painter scales them back.
                                .map(|(p, r)| {
                                    let q = viewport.to_pane(p);
                                    (xy(q.x / zoom, q.y / zoom), r)
                                })
                                .collect();
                            paint::paint_ink(&outline, origin, zoom, color, None, window);
                        }
                        Some(Drag::Select { start, current, .. }) => {
                            let (a, b) = (viewport.to_pane(*start), viewport.to_pane(*current));
                            let area = Bounds::from_corners(
                                point(origin.x + px(a.x.min(b.x)), origin.y + px(a.y.min(b.y))),
                                point(origin.x + px(a.x.max(b.x)), origin.y + px(a.y.max(b.y))),
                            );
                            window.paint_quad(quad(
                                area,
                                px(0.),
                                colors.selection_fill,
                                px(1.),
                                colors.selected,
                                BorderStyle::default(),
                            ));
                        }
                        _ => {}
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };

        let panning = matches!(&self.drag, Some(Drag::Pan { moved: true, .. }));
        let panels: Vec<Div> = self
            .panels
            .iter()
            .map(|(position, render)| {
                let m = px(15.);
                let base = div()
                    .absolute()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_scroll(|_, _, cx| cx.stop_propagation())
                    .child(render(self, window, cx));
                match position {
                    PanelPosition::TopLeft => base.top(m).left(m),
                    PanelPosition::TopRight => base.top(m).right(m),
                    PanelPosition::BottomLeft => base.bottom(m).left(m),
                    PanelPosition::BottomRight => base.bottom(m).right(m),
                    // Centered by a full-width row that takes no presses of its own.
                    PanelPosition::TopCenter | PanelPosition::BottomCenter => {
                        let row = div().absolute().left_0().w_full().flex().justify_center();
                        let inner = div()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_scroll(|_, _, cx| cx.stop_propagation())
                            .child(render(self, window, cx));
                        if *position == PanelPosition::TopCenter {
                            row.top(m).child(inner)
                        } else {
                            row.bottom(m).child(inner)
                        }
                    }
                }
            })
            .collect();

        // The flow's keys live on a layer of their own, beside the nodes and panels rather than
        // around them: they're only on the way of a key while the flow itself has the focus,
        // never while a text field in a node or a panel does.
        let keys = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|flow, _: &DeleteSelected, _, cx| {
                let before = (flow.nodes.len(), flow.edges.len());
                if !(flow.options.deletable
                    && (flow.delete_selected(cx) || flow.delete_focused(cx)))
                {
                    cx.propagate();
                    return;
                }
                let gone = (
                    before.0.saturating_sub(flow.nodes.len()),
                    before.1.saturating_sub(flow.edges.len()),
                );
                let count = |n: usize, one: &str| {
                    if n == 1 {
                        format!("1 {one}")
                    } else {
                        format!("{n} {one}s")
                    }
                };
                let words = match gone {
                    (0, e) => format!("Deleted {}", count(e, "edge")),
                    (n, 0) => format!("Deleted {}", count(n, "node")),
                    (n, e) => format!("Deleted {} and {}", count(n, "node"), count(e, "edge")),
                };
                flow.announce(words, cx);
            }))
            .on_action(cx.listener(|flow, _: &FocusPrevious, _, cx| {
                if !flow.move_keyboard(false, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &SelectFocused, _, cx| {
                if !flow.select_focused(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &SelectAll, _, cx| flow.select_all(cx)))
            .on_action(cx.listener(|flow, _: &Cancel, _, cx| {
                if !flow.cancel(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &ZoomIn, _, cx| flow.zoom_in(cx)))
            .on_action(cx.listener(|flow, _: &ZoomOut, _, cx| flow.zoom_out(cx)))
            .on_action(cx.listener(|flow, _: &ZoomReset, _, cx| flow.zoom_to(1., cx)))
            .on_action(cx.listener(|flow, _: &FitView, _, cx| flow.fit_view(cx)))
            .on_action(cx.listener(|flow, _: &Undo, _, cx| {
                if !flow.undo(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &Redo, _, cx| {
                if !flow.redo(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &FrameSelection, _, cx| {
                if flow.frame_selection("Frame", cx).is_none() {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &Unframe, _, cx| {
                let frames: Vec<NodeId> = flow
                    .nodes
                    .iter()
                    .filter(|n| n.selected && is_frame(n))
                    .map(|n| n.id.clone())
                    .collect();
                if frames.is_empty() {
                    cx.propagate();
                }
                for id in frames {
                    flow.unframe(&id, cx);
                }
            }))
            .on_action(cx.listener(|flow, _: &OpenLink, _, cx| {
                let opened = flow
                    .only_selected()
                    .is_some_and(|id| flow.open_link(&id, cx));
                if !opened {
                    cx.propagate();
                }
            }))
            // Tab adds a child in a tree with a node selected; otherwise it moves the keyboard
            // to the next node or edge, and past the last one out of the canvas.
            .on_action(cx.listener(|flow, _: &AddChild, _, cx| {
                if !flow.add_child(cx) && !flow.move_keyboard(true, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &AddSibling, _, cx| {
                if !flow.add_sibling(cx) && !flow.select_focused(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|f, _: &MoveLeft, _, cx| f.nudge_or_pass(-1., 0., false, cx)))
            .on_action(cx.listener(|f, _: &MoveRight, _, cx| f.nudge_or_pass(1., 0., false, cx)))
            .on_action(cx.listener(|f, _: &MoveUp, _, cx| f.nudge_or_pass(0., -1., false, cx)))
            .on_action(cx.listener(|f, _: &MoveDown, _, cx| f.nudge_or_pass(0., 1., false, cx)))
            .on_action(cx.listener(|f, _: &MoveLeftFar, _, cx| f.nudge_or_pass(-1., 0., true, cx)))
            .on_action(cx.listener(|f, _: &MoveRightFar, _, cx| f.nudge_or_pass(1., 0., true, cx)))
            .on_action(cx.listener(|f, _: &MoveUpFar, _, cx| f.nudge_or_pass(0., -1., true, cx)))
            .on_action(cx.listener(|f, _: &MoveDownFar, _, cx| f.nudge_or_pass(0., 1., true, cx)))
            .child(pane)
            .children(self.focus_ring(&colors))
            // The live region: what the canvas says, read out as it changes.
            .child(crate::a11y::Live::new(
                div()
                    .id("flow-announcer")
                    .absolute()
                    .size_0()
                    .overflow_hidden()
                    .role(Role::Status)
                    .aria_label(self.announcement.clone()),
            ));

        // Each edge drawn, as assistive technology gets it: a group named for it, where its line
        // was last painted. Edges are painted, not elements, so they're nodes of the canvas's own.
        let edge_names: Vec<(EdgeId, SharedString)> = self
            .edges
            .iter()
            .filter(|e| !e.hidden)
            .map(|e| (e.id.clone(), self.edge_name(e)))
            .collect();
        for (_, name) in &edge_names {
            gpui_core::audit("FlowEdge", Role::Group, Some(name));
        }
        let edge_nodes = {
            let shared = self.shared.clone();
            let scale = window.scale_factor();
            move |builder: &mut A11ySubtreeBuilder| {
                let lines = shared.edges.borrow();
                let pane = shared.pane.get().origin;
                for (id, name) in &edge_names {
                    let Some(line) = lines.iter().find(|l| &l.id == id) else {
                        continue;
                    };
                    let mut node = accesskit::Node::new(Role::Group);
                    node.set_label(name.to_string());
                    node.set_role_description("edge");
                    if !line.points.is_empty() {
                        let (ox, oy) = (f32::from(pane.x), f32::from(pane.y));
                        let fold = |pick: fn(f32, f32) -> f32, of: fn(&XY) -> f32, start: f32| {
                            line.points.iter().map(of).fold(start, pick)
                        };
                        let (x0, x1) = (
                            fold(f32::min, |p| p.x, f32::MAX),
                            fold(f32::max, |p| p.x, f32::MIN),
                        );
                        let (y0, y1) = (
                            fold(f32::min, |p| p.y, f32::MAX),
                            fold(f32::max, |p| p.y, f32::MIN),
                        );
                        node.set_bounds(accesskit::Rect {
                            x0: ((ox + x0) * scale) as f64,
                            y0: ((oy + y0) * scale) as f64,
                            x1: ((ox + x1) * scale) as f64,
                            y1: ((oy + y1) * scale) as f64,
                        });
                    }
                    builder.push_child(builder.synthetic_node_id(id.as_ref()), node);
                }
            }
        };
        gpui_core::audit("Flow", Role::Group, Some(&self.options.aria_label));

        div()
            .id("flow")
            .role(Role::Group)
            .aria_label(self.options.aria_label.clone())
            .a11y_synthetic_children(edge_nodes)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(colors.background)
            .when(self.options.pan_on_drag, |d| {
                d.cursor(if panning {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
            })
            .when(
                self.tool != Tool::Select && self.options.pointer_mode == PointerMode::Edit,
                |d| d.cursor(CursorStyle::Crosshair),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pane_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::pane_mouse_down))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|flow, event, _, cx| flow.pane_context_menu(event, cx)),
            )
            .on_scroll(cx.listener(|flow, event, _, cx| flow.scroll(event, cx)))
            .on_drop(cx.listener(|flow, paths: &ExternalPaths, window, cx| {
                if flow.options.pointer_mode == PointerMode::Off {
                    return;
                }
                let position = flow.screen_to_flow(window.mouse_position());
                cx.emit(FlowEvent::FilesDropped {
                    paths: paths.paths().to_vec(),
                    position,
                });
            }))
            .on_pinch(cx.listener(|flow, event, _, cx| flow.pinch(event, cx)))
            .child(keys)
            .children(layers)
            .child(overlay)
            .when(self.minimap, |d| d.child(self.render_minimap(colors, cx)))
            .when(self.controls, |d| d.child(self.render_controls(colors, cx)))
            .children(self.render_node_toolbar(window, cx))
            .children(panels)
    }
}

impl Flow {
    /// An arrow key: moves the selected nodes, or with none to move lets the key go on.
    fn nudge_or_pass(&mut self, dx: f32, dy: f32, far: bool, cx: &mut Context<Self>) {
        // In a tree the layout places nodes: the arrows move the selection instead.
        let done = if self.tree_layout().is_some() {
            self.select_toward(xy(dx, dy), cx)
        } else {
            self.nudge(dx, dy, far, cx)
        };
        if !done {
            cx.propagate();
        }
    }
}

/// The distance between two points of the window, in points.
fn distance(a: Point<Pixels>, b: Point<Pixels>) -> f32 {
    (a - b).magnitude() as f32
}

/// The time by GPUI's clock: the real one in an app, and one a test moves itself, so
/// animations step the same however fast the machine draws.
fn now(cx: &App) -> Instant {
    cx.background_executor().now()
}

/// How far the viewport moves a sixtieth of a second for a drag's pointer at `at` in a pane of
/// `size`: `speed` at the pane's edge (or past it), less further in, and nothing more than 40
/// in from it. React Flow's `calcAutoPan`.
fn auto_pan_velocity(at: XY, size: (f32, f32), speed: f32) -> (f32, f32) {
    const REACH: f32 = 40.;
    let along = |value: f32, max: f32| {
        if value < REACH {
            (REACH - value).clamp(1., REACH) / REACH
        } else if value > max - REACH {
            -(value - (max - REACH)).clamp(1., REACH) / REACH
        } else {
            0.
        }
    };
    (along(at.x, size.0) * speed, along(at.y, size.1) * speed)
}

/// Whether a node is a frame: the built-in kind that holds other nodes.
fn is_frame(node: &Node) -> bool {
    node.kind == "frame"
}

/// A routed edge's line: round the nodes in its way, as it was last worked out unless its ends
/// or a node near it moved. None to take the usual route: nothing in the way, or too much.
fn routed_path(
    shared: &Shared,
    edge: &EdgeDraw,
    from: XY,
    from_side: Side,
    to: XY,
    to_side: Side,
    obstacles: &[Rect],
) -> Option<EdgePath> {
    use std::hash::{Hash, Hasher};
    // Only what's near the edge's two ends counts.
    let reach = 160.;
    let area = Rect::from_corners(from, to);
    let area = Rect::new(
        area.x - reach,
        area.y - reach,
        area.w + reach * 2.,
        area.h + reach * 2.,
    );
    let near: Vec<Rect> = obstacles
        .iter()
        .filter(|r| r.intersects(&area))
        .copied()
        .collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut put = |v: f32| ((v * 2.).round() as i64).hash(&mut hasher);
    for v in [from.x, from.y, to.x, to.y] {
        put(v);
    }
    for r in &near {
        for v in [r.x, r.y, r.w, r.h] {
            put(v);
        }
    }
    (from_side as u8, to_side as u8).hash(&mut hasher);
    let key = hasher.finish();
    let cached = shared
        .routes
        .borrow()
        .get(&edge.id)
        .filter(|(k, _)| *k == key)
        .map(|(_, points)| points.clone());
    let points = match cached {
        Some(points) => points,
        None => {
            let points = crate::routing::route(from, from_side, to, to_side, &near, 12.);
            shared
                .routes
                .borrow_mut()
                .insert(edge.id.clone(), (key, points.clone()));
            points
        }
    }?;
    let radius = if edge.kind == EdgeKind::SmoothStep {
        5.
    } else {
        0.
    };
    Some(rounded_path(&points, radius, longest_middle(&points)))
}

#[cfg(test)]
mod ink_cache_tests {
    use super::*;
    use crate::ink::{InkPoint, InkStroke, InkTool};
    // The built-in #[test], not GPUI's, which the glob above brings in.
    use std::prelude::v1::test;

    fn drawn(cx: &mut gpui::VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
    }

    #[gpui::test]
    fn removed_strokes_leave_the_cache(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let stroke = InkStroke::anchored(
            InkTool::Ball,
            0x0000_00ff,
            2.,
            vec![
                InkPoint::at(0., 0., 0., 2.),
                InkPoint::at(60., 12., 0.1, 2.),
            ],
        );
        let (one, two) = (Node::ink("s1", &stroke), Node::ink("s2", &stroke));
        let (flow, cx) = cx.add_window_view(move |_, cx| Flow::new(cx).with_nodes(vec![one, two]));
        drawn(cx);
        let cached = |cx: &mut gpui::VisualTestContext| {
            flow.read_with(cx, |f, _| f.shared.ink.borrow().len())
        };
        assert_eq!(cached(cx), 2);
        // Removed here, and by an edit from elsewhere.
        flow.update(cx, |f, cx| {
            f.remove_nodes(&["s1".into()], cx);
        });
        assert_eq!(cached(cx), 1);
        flow.update(cx, |f, cx| {
            f.apply(&[Op::RemoveNode { id: "s2".into() }], cx)
        });
        assert_eq!(cached(cx), 0);
    }
}
