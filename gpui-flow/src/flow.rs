//! The flow itself: an entity that holds the nodes, the edges and the viewport, draws them,
//! and turns the mouse and keyboard into moves, selections, connections and deletions.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::geometry::{distance_to_polyline, edge_path};
use crate::graph;
use crate::node::{
    EdgeLine, Measured, NodeContext, NodeInput as _, NodeRenderer, Reach, Shared, TouchTarget,
    Zoomed, built_in, measure_node, touch_target, units,
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
        /// In a tree layout, adds a child to the selected node.
        AddChild,
        /// In a tree layout, adds a sibling after the selected node.
        AddSibling,
        /// Moves the selected nodes four steps.
        MoveLeftFar,
        MoveRightFar,
        MoveUpFar,
        MoveDownFar,
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
        KeyBinding::new("tab", AddChild, c),
        KeyBinding::new("enter", AddSibling, c),
        KeyBinding::new("shift-left", MoveLeftFar, c),
        KeyBinding::new("shift-right", MoveRightFar, c),
        KeyBinding::new("shift-up", MoveUpFar, c),
        KeyBinding::new("shift-down", MoveDownFar, c),
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
    /// Below this zoom, nodes are drawn as plain boxes of their size and colors; 0 never does.
    pub simplify_below_zoom: f32,
    /// What presses, scrolling and pinches on the canvas do: edit it, only move about it, or
    /// nothing, leaving them to a view laid over it.
    pub pointer_mode: PointerMode,
    /// How large (in points) the area is that a finger can take a handle or a resizer's grip
    /// by, at least: Apple's 44 by default. A finger takes an edge from up to half that away.
    pub touch_target_size: f32,
    /// Where nodes go: where they're put, or laid out as a tree read from the edges (a mind
    /// map). In a tree, tab adds a child to the selected node and enter a sibling, the arrows
    /// move the selection, a node dropped on another becomes its child, and delete takes a
    /// node's subtree with it.
    pub layout: Layout,
    /// How far (in points on screen) a pressed node must move before it counts as a drag.
    pub node_drag_threshold: f32,
    /// How long the viewport takes to get where the controls, a fit or a double click send it.
    pub transition: Duration,
    /// How much room a fit leaves around the nodes: 0.1 shows an area a tenth larger.
    pub fit_padding: f32,
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
            simplify_below_zoom: 0.3,
            pointer_mode: PointerMode::Edit,
            touch_target_size: 44.,
            layout: Layout::Free,
            node_drag_threshold: 1.,
            transition: Duration::ZERO,
            fit_padding: 0.1,
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

/// One handle of one node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleRef {
    pub node: NodeId,
    pub handle: Option<SharedString>,
    pub kind: HandleKind,
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
    NodeClick(NodeId),
    NodeDoubleClick(NodeId),
    /// The mouse began moving these nodes; [`FlowEvent::NodeDrag`] follows each move, and
    /// [`FlowEvent::NodeDragStop`] the release, or escape (which puts them back first).
    NodeDragStart(Vec<NodeId>),
    NodeDrag(Vec<NodeId>),
    NodeDragStop(Vec<NodeId>),
    /// The arrow keys moved these nodes.
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
    /// A node was added with tab or enter in a tree layout, and selected: where the host starts
    /// editing its text.
    NodeAdd {
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
        moved: bool,
    },
    Minimap,
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
    valid_connection: Option<ConnectionCheck>,
    delete_check: Option<DeleteCheck>,
    transform_listener: Option<TransformListener>,
    /// Where each node is in `nodes`, built on the first lookup after the list changes: a
    /// frame looks nodes up by id several times each.
    index: RefCell<Option<HashMap<NodeId, usize>>>,
    shared: Rc<Shared>,
    focus: FocusHandle,
    drag: Option<Drag>,
    /// The button that began the drag: only its release ends it.
    drag_button: MouseButton,
    transition: Option<Transition>,
    /// The padding of a fit that's waiting for the nodes' sizes.
    pending_fit: Option<f32>,
    /// False until a fit asked for before the first frame has happened; nothing shows until then.
    ready: bool,
    /// Runs while an animated edge is on screen, to redraw the dashes.
    ticker: Option<Task<()>>,
    started: Instant,
    last_selection: (Vec<NodeId>, Vec<EdgeId>),
    touch: Option<TouchPress>,
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
}

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
            valid_connection: None,
            delete_check: None,
            transform_listener: None,
            index: RefCell::new(None),
            shared: Rc::default(),
            focus: cx.focus_handle(),
            drag: None,
            drag_button: MouseButton::Left,
            transition: None,
            pending_fit: None,
            ready: true,
            ticker: None,
            started: Instant::now(),
            last_selection: Default::default(),
            touch: None,
            last_tap: None,
            tree: Tree::default(),
            folded: HashSet::new(),
            placed: HashMap::new(),
            laid_out: None,
            new_node: None,
        }
    }

    // Setting a flow up.

    pub fn with_nodes(mut self, mut nodes: Vec<Node>) -> Self {
        nodes.iter_mut().for_each(sanitize_node);
        self.nodes = nodes;
        self.reindex();
        self.last_selection = (self.selected_nodes(), self.selected_edges());
        self
    }

    pub fn with_edges(mut self, mut edges: Vec<Edge>) -> Self {
        edges.iter_mut().for_each(sanitize_edge);
        self.edges = edges;
        self.last_selection = (self.selected_nodes(), self.selected_edges());
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
            let mut index = HashMap::with_capacity(self.nodes.len());
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
    fn shown(&self, node: &Node) -> bool {
        !node.hidden && !self.folded.contains(&node.id)
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
    fn lay_out_tree(&mut self, window: &mut Window) {
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
        let now = Instant::now();
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
        let parent_node = parent.as_ref().and_then(|p| self.node(p)).cloned();
        let mut node = match &self.new_node {
            Some(make) => make(parent_node.as_ref(), self),
            None => {
                let mut n = 1;
                while self.node(&format!("node-{n}")).is_some() {
                    n += 1;
                }
                let kind = parent_node
                    .as_ref()
                    .map_or(SharedString::from("default"), |p| p.kind.clone());
                Node::new(format!("node-{n}"), XY::default())
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
            let connection = Connection {
                source: parent.clone(),
                source_handle: None,
                target: id.clone(),
                target_handle: None,
            };
            let d = &self.options.edge_defaults;
            let edge = Edge {
                id: graph::unique_edge_id(&connection, &self.edges),
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
                let connection = Connection {
                    source: parent.clone(),
                    source_handle: None,
                    target: node.clone(),
                    target_handle: None,
                };
                let edge = Edge::new(
                    graph::unique_edge_id(&connection, &self.edges),
                    parent.clone(),
                    node.clone(),
                );
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

    /// The level a node is drawn at, as React Flow reckons it: its own `z_index`, raised by
    /// 1000 while it's selected, and always above its parent's.
    fn node_z(&self, node: &Node) -> i64 {
        let own = |n: &Node| {
            n.z_index as i64
                + if self.options.elevate_nodes_on_select && n.selected {
                    1000
                } else {
                    0
                }
        };
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
    fn edge_z(&self, edge: &Edge, source: &Node, target: &Node) -> i64 {
        let o = &self.options;
        let own = edge.z_index as i64
            + if o.elevate_edges_on_select && edge.selected {
                1000
            } else {
                0
            };
        let lifted = |n: &Node| n.parent.is_some() || (o.elevate_nodes_on_select && n.selected);
        let ends = [source, target]
            .into_iter()
            .filter(|n| lifted(n))
            .map(|n| self.node_z(n))
            .max()
            .unwrap_or(0);
        own + ends
    }

    // Changing a flow.

    pub fn set_nodes(&mut self, mut nodes: Vec<Node>, cx: &mut Context<Self>) {
        nodes.iter_mut().for_each(sanitize_node);
        let old = std::mem::replace(&mut self.nodes, nodes);
        self.reindex();
        self.forget_changed(&old);
        self.forget_removed();
        self.selection_changed(cx);
        cx.notify();
    }

    pub fn set_edges(&mut self, mut edges: Vec<Edge>, cx: &mut Context<Self>) {
        edges.iter_mut().for_each(sanitize_edge);
        self.edges = edges;
        self.selection_changed(cx);
        cx.notify();
    }

    pub fn add_node(&mut self, mut node: Node, cx: &mut Context<Self>) {
        sanitize_node(&mut node);
        self.nodes.push(node);
        self.reindex();
        self.selection_changed(cx);
        cx.notify();
    }

    /// Adds the edge unless one already joins the same two handles; says whether it did.
    pub fn add_edge(&mut self, mut edge: Edge, cx: &mut Context<Self>) -> bool {
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
        self.transition = None;
        self.drag = None;
        self.forget_changed(&old);
        self.forget_removed();
        self.selection_changed(cx);
        let viewport = self.sane_viewport(viewport);
        self.move_viewport(viewport, cx);
        cx.notify();
    }

    /// Lets go of the measurements of nodes that are gone.
    fn forget_removed(&mut self) {
        let ids: HashSet<&NodeId> = self.nodes.iter().map(|n| &n.id).collect();
        self.shared
            .measured
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

    /// Makes the edge for a connection, as a drag between handles does, but without telling
    /// subscribers. None when a node is missing, the flow's connection check refuses it, or the
    /// edge already exists.
    pub fn connect(&mut self, connection: Connection, cx: &mut Context<Self>) -> Option<Edge> {
        if self.node(&connection.source).is_none()
            || self.node(&connection.target).is_none()
            || !self.connection_allowed(&connection)
        {
            return None;
        }
        let d = &self.options.edge_defaults;
        let edge = Edge {
            id: graph::unique_edge_id(&connection, &self.edges),
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
                start: Instant::now(),
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
            PointerMode::Edit => None,
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
        });
        cx.notify();
        true
    }

    /// Starts resizing a node by one of its resizer's corners. Says whether the press was the
    /// flow's.
    pub(crate) fn resize_mouse_down(
        &mut self,
        node: &NodeId,
        corner: ResizeCorner,
        min: (f32, f32),
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
        cx.emit(if event.click_count >= 2 {
            FlowEvent::EdgeDoubleClick(id)
        } else {
            FlowEvent::EdgeClick(id)
        });
        cx.notify();
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
                && at.elapsed() <= DOUBLE_TAP
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
            TouchTarget::Resize { node, corner, min } => {
                self.resize_mouse_down(node, *corner, *min, &down, window, cx)
            }
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
                    .then(|| (Instant::now(), event.position, press.node.clone()));
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
            .find(|n| self.node_rect(&n.id).is_some_and(|r| r.contains(at)))
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
        let pane = self.shared.pane_point(event.position);
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
        match &mut drag {
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
                ..
            } => {
                if !*started {
                    *started = true;
                    cx.emit(FlowEvent::ConnectStart {
                        node: from.node.clone(),
                        handle: from.handle.clone(),
                        kind: from.kind,
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
                moved,
                ..
            } => {
                if !*moved {
                    *moved = true;
                    cx.emit(FlowEvent::NodeResizeStart(node.clone()));
                }
                let delta = pointer - *start;
                let left = matches!(corner, ResizeCorner::TopLeft | ResizeCorner::BottomLeft);
                let top = matches!(corner, ResizeCorner::TopLeft | ResizeCorner::TopRight);
                let w = (size.0 + if left { -delta.x } else { delta.x }).max(min.0);
                let h = (size.1 + if top { -delta.y } else { delta.y }).max(min.1);
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
            Drag::Minimap => self.minimap_center(event.position, cx),
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
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag {
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
                from,
                snap,
                started,
                pointer,
                ..
            } => {
                if started {
                    let to = snap.as_ref().map(|s| s.to.clone());
                    let made = snap
                        .filter(|s| s.valid)
                        .and_then(|s| self.connect(s.connection, cx));
                    let connected = made.is_some();
                    if let Some(edge) = made {
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
        if self.nodes.iter().all(|n| !n.selected) && self.edges.iter().all(|e| !e.selected) {
            return false;
        }
        self.deselect_all(cx);
        cx.notify();
        true
    }

    /// Drops what's being dragged, putting back what it changed. Says whether anything was.
    fn cancel_drag(&mut self, cx: &mut Context<Self>) -> bool {
        match self.drag.take() {
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

    /// Moves a transition along, and asks for another frame until it's done.
    fn step_transition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.transition else {
            return;
        };
        let p = (t.start.elapsed().as_secs_f32() / t.duration.as_secs_f32()).min(1.);
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
        let (w, h) = self.pane_size();
        let margin = OVERDRAW / self.viewport.zoom;
        let visible = self.viewport.visible(w, h);
        let visible = Rect::new(
            visible.x - margin,
            visible.y - margin,
            visible.w + margin * 2.,
            visible.h + margin * 2.,
        );
        let cull = self.options.only_render_visible && w > 0.;
        let measured = self.shared.measured.borrow();
        let mut order: Vec<(i64, usize)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| self.shown(n) && self.index_of(&n.id) == Some(*i))
            .filter(|(_, n)| {
                !cull
                    || !measured.get(&n.id).is_some_and(|m| m.size.is_some())
                    || self.node_rect(&n.id).is_none_or(|r| r.intersects(&visible))
            })
            .map(|(i, n)| (self.node_z(n), i))
            .collect();
        order.sort();
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
            depth: self.tree_layout().map(|_| self.tree.depth(&node.id)),
            children: self.tree.children_of(&node.id).len(),
            collapsed: node.collapsed,
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
            .when(editing && self.draggable(node), |d| {
                d.cursor(if dragging {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
            })
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
            .child(touch_target(
                this.clone(),
                TouchTarget::Node(node.id.clone()),
                px(0.),
                Reach::Around,
            ))
            .when_some(self.collapse_chip(node, &ctx, this), |d, chip| {
                d.child(chip)
            })
            .child(measure_node(
                self.shared.clone(),
                this.entity_id(),
                node.id.clone(),
            ))
            .children(content);
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
    fn edge_draws(&self, colors: &FlowColors) -> Vec<EdgeDraw> {
        self.edges
            .iter()
            .filter(|e| !e.hidden)
            .filter_map(|e| {
                let (source, target) = (self.node(&e.source)?, self.node(&e.target)?);
                if !self.shown(source) || !self.shown(target) {
                    return None;
                }
                Some(EdgeDraw {
                    id: e.id.clone(),
                    z: self.edge_z(e, source, target),
                    kind: e.kind.clone(),
                    source: End {
                        node: source.id.clone(),
                        at: self.absolute_position(&source.id)?,
                        handle: e.source_handle.clone(),
                    },
                    target: End {
                        node: target.id.clone(),
                        at: self.absolute_position(&target.id)?,
                        handle: e.target_handle.clone(),
                    },
                    label: e.label.clone(),
                    animated: e.animated,
                    marker_start: e.marker_start,
                    marker_end: e.marker_end,
                    color: if e.selected {
                        colors.selected
                    } else {
                        e.color.map(|c| rgb(c).into()).unwrap_or(colors.edge)
                    },
                    width: e.stroke_width.unwrap_or(1.) + if e.selected { 0.5 } else { 0. },
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
        let phase = self.started.elapsed().as_secs_f32() * DASH_SPEED * zoom;
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
                    let path = edge_path(&edge.kind, from, from_side, to, to_side);
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
                        ink.dashed(&line, width, DASH * zoom, phase, edge.color);
                        animated = true;
                    } else {
                        ink.line(&line, width, edge.color);
                    }
                    let n = line.len();
                    if n >= 2 {
                        if let Some(marker) = &edge.marker_end {
                            ink.marker(marker, line[n - 1], line[n - 2], zoom, width, edge.color);
                        }
                        if let Some(marker) = &edge.marker_start {
                            ink.marker(marker, line[0], line[1], zoom, width, edge.color);
                        }
                    }
                    if let Some(label) = &edge.label {
                        labels.push((label, viewport.to_pane(path.label)));
                    }
                    lines.push(EdgeLine {
                        id: edge.id.clone(),
                        z,
                        reach: (edge.reach * zoom).max(MIN_REACH),
                        points: line,
                    });
                }
                // Every line first, as a path or two, and then the labels over them.
                ink.paint(window);
                for (label, at) in labels {
                    paint::paint_label(origin, label, at, zoom, &colors, window, cx);
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
    /// Half the interaction width, in flow units.
    reach: f32,
    /// Where a tree edge meets its nodes, set by the layout rather than by handles.
    anchors: Option<((XY, Side), (XY, Side))>,
}

/// A node drawn as a plain box, in pane coordinates.
struct NodeBox {
    rect: Rect,
    fill: Hsla,
    border: Hsla,
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
                window.paint_quad(quad(
                    area,
                    px(1.),
                    b.fill,
                    px(1.),
                    b.border,
                    BorderStyle::default(),
                ));
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
    measured: &HashMap<NodeId, Measured>,
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
        self.enforce_extents();
        self.lay_out_tree(window);
        self.try_pending_fit(cx);
        self.step_transition(window, cx);
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
        let mut draws = if ready {
            self.edge_draws(&colors)
        } else {
            Vec::new()
        };
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
        for (z, i) in self.draw_order() {
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
                boxes.push(NodeBox {
                    rect: Rect::new(a.x, a.y, rect.w * zoom, rect.h * zoom),
                    fill: if node.kind == "group" {
                        colors.group
                    } else {
                        colors.node
                    },
                    border: if node.selected {
                        colors.selected
                    } else {
                        colors.node_border
                    },
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
        let overlay = {
            let drag = self.drag.clone();
            let line_kind = self.options.connection_line.clone();
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let origin = bounds.origin;
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
                if !(flow.options.deletable && flow.delete_selected(cx)) {
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
            .on_action(cx.listener(|flow, _: &AddChild, _, cx| {
                if !flow.add_child(cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|flow, _: &AddSibling, _, cx| {
                if !flow.add_sibling(cx) {
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
            .child(pane);

        div()
            .id("flow")
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
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pane_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::pane_mouse_down))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|flow, event, _, cx| flow.pane_context_menu(event, cx)),
            )
            .on_scroll(cx.listener(|flow, event, _, cx| flow.scroll(event, cx)))
            .on_pinch(cx.listener(|flow, event, _, cx| flow.pinch(event, cx)))
            .child(keys)
            .children(layers)
            .child(overlay)
            .when(self.minimap, |d| d.child(self.render_minimap(colors, cx)))
            .when(self.controls, |d| d.child(self.render_controls(colors, cx)))
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
