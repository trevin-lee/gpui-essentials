//! What a node is drawn with: the context its renderer gets, the handles and resizer it can
//! place, the built-in node kinds, and the element that makes a node follow the zoom.
//!
//! GPUI has no transform for elements, so a node doesn't scale the way a CSS-transformed one
//! does. Instead the flow sets the size of a rem under each node to 16 points times the zoom,
//! and nodes size themselves in [`units`]: lengths in flow units, which are rems underneath.
//! Anything a node sizes in plain pixels (a hairline border, say) stays that size at every zoom.
//! Text is the exception to sizing in units: it takes its size from [`NodeContext::font_size`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;

use crate::flow::Flow;
use crate::theme::FlowColors;
use crate::types::{HandleKind, Node, NodeId, Rect, ResizeCorner, Side, Viewport, XY, xy};

/// A length in flow units: one point at zoom 1, and scaled with the zoom inside a node.
pub fn units(n: f32) -> Rems {
    rems(n / 16.)
}

/// A text size of `n` flow units at this zoom, snapped to half a pixel.
///
/// Text sized in [`units`] follows the zoom exactly, and so asks for a size no frame before it
/// used: while the zoom changes, every frame makes its fonts and draws its glyphs from nothing,
/// which costs more than the rest of the frame together. Snapped sizes repeat, and repeated
/// sizes are cached. The text is at most a quarter of a pixel off the size its box expects.
pub fn font_size(n: f32, zoom: f32) -> Pixels {
    px(((n * zoom * 2.).round() / 2.).max(1.))
}

/// The size of a handle's dot, in flow units.
pub const HANDLE_SIZE: f32 = 8.;

/// Draws a node. It's called on every frame the node is on screen, while the flow is being
/// drawn: read the flow from the `&Flow` it's given, never through the entity (which is busy).
pub type NodeRenderer = Rc<dyn Fn(&Node, &NodeContext, &Flow, &mut Window, &mut App) -> AnyElement>;

/// Ways to keep the canvas from taking input meant for something inside a node, as React
/// Flow's `nodrag` and `nowheel` classes do.
///
/// A child of a node that takes presses itself (a button, a text field, a slider) should be
/// `nodrag`: pressing it then neither selects nor drags the node, nor takes the focus from it,
/// and a finger on it taps it rather than dragging the node. One that scrolls should be
/// `nowheel`, so scrolling over it doesn't zoom or pan the canvas.
pub trait NodeInput: InteractiveElement + Sized {
    fn nodrag(self) -> Self {
        // Blocking the pointer for what's behind keeps the node's touch target from claiming a
        // touch that starts here, so it ends as an ordinary tap.
        self.block_mouse_except_scroll()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    fn nowheel(self) -> Self {
        self.on_scroll(|_, _, cx| cx.stop_propagation())
    }
}

impl<E: InteractiveElement + Sized> NodeInput for E {}

/// What a touch that starts on part of the canvas takes hold of.
#[derive(Clone, Debug)]
pub(crate) enum TouchTarget {
    /// The pane, or an edge or a node drawn as a box on it.
    Pane,
    Node(NodeId),
    Handle {
        node: NodeId,
        handle: Option<SharedString>,
        kind: HandleKind,
    },
    Resize {
        node: NodeId,
        corner: ResizeCorner,
        min: (f32, f32),
    },
}

/// Which way a touch target may grow past its element to reach its least size.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Reach {
    /// Every way.
    Around,
    /// Away from the node only, for a handle on this side: grown into the node, it would
    /// take touches meant for the node itself, which may be no taller than the target.
    Out(Side),
    /// Out of this corner only, for a resizer's grip.
    Corner(ResizeCorner),
}

/// An area, at least `min` points across around its parent (as `reach` allows), where a touch
/// that starts is offered to the flow for `target`: one the flow takes is a drag of the node,
/// a connection or a resize, rather than a pan of the canvas.
pub(crate) fn touch_target(
    flow: WeakEntity<Flow>,
    target: TouchTarget,
    min: Pixels,
    reach: Reach,
) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let (w, h) = (bounds.size.width.max(min), bounds.size.height.max(min));
            let c = bounds.center();
            let (mut left, mut top) = (c.x - w / 2., c.y - h / 2.);
            let (mut right, mut bottom) = (left + w, top + h);
            let inner = |side: Side,
                         left: &mut Pixels,
                         top: &mut Pixels,
                         right: &mut Pixels,
                         bottom: &mut Pixels| {
                match side {
                    Side::Top => *bottom = (*bottom).min(bounds.bottom()),
                    Side::Bottom => *top = (*top).max(bounds.top()),
                    Side::Left => *right = (*right).min(bounds.right()),
                    Side::Right => *left = (*left).max(bounds.left()),
                }
            };
            match reach {
                Reach::Around => {}
                Reach::Out(side) => inner(side, &mut left, &mut top, &mut right, &mut bottom),
                Reach::Corner(corner) => {
                    let (h, v) = match corner {
                        ResizeCorner::TopLeft => (Side::Left, Side::Top),
                        ResizeCorner::TopRight => (Side::Right, Side::Top),
                        ResizeCorner::BottomLeft => (Side::Left, Side::Bottom),
                        ResizeCorner::BottomRight => (Side::Right, Side::Bottom),
                    };
                    inner(h, &mut left, &mut top, &mut right, &mut bottom);
                    inner(v, &mut left, &mut top, &mut right, &mut bottom);
                }
            }
            let area = Bounds::from_corners(point(left, top), point(right, bottom));
            window.insert_hitbox(area, HitboxBehavior::Normal)
        },
        move |_, hitbox, window, _| {
            let (hitbox, flow, target) = (hitbox.clone(), flow.clone(), target.clone());
            window.on_mouse_event(move |event: &TouchDragEvent, phase, window, cx| {
                if !phase.bubble()
                    || event.phase != TouchPhase::Started
                    || window.default_prevented()
                    || !hitbox.is_hovered(window)
                {
                    return;
                }
                let claimed = flow
                    .update(cx, |flow, cx| {
                        flow.touch_start(&target, event.start_position, window, cx)
                    })
                    .unwrap_or(false);
                if claimed {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A handle as it was last laid out, relative to its node's top left corner.
#[derive(Clone, Debug)]
pub(crate) struct HandleBounds {
    pub id: Option<SharedString>,
    pub kind: HandleKind,
    pub position: Side,
    pub rect: Rect,
    pub connectable: bool,
}

impl HandleBounds {
    /// Where an edge meets the handle: the middle of its outer side.
    pub fn anchor(&self) -> XY {
        let c = self.rect.center();
        match self.position {
            Side::Left => xy(self.rect.x, c.y),
            Side::Right => xy(self.rect.right(), c.y),
            Side::Top => xy(c.x, self.rect.y),
            Side::Bottom => xy(c.x, self.rect.bottom()),
        }
    }
}

/// An edge's line as it was last painted.
#[derive(Clone, Debug)]
pub(crate) struct EdgeLine {
    pub id: SharedString,
    /// The level it was drawn at, among the nodes' levels.
    pub z: i64,
    /// How near (in points) a press must be to take it.
    pub reach: f32,
    /// In pane coordinates.
    pub points: Vec<XY>,
}

/// What a node measured the last time it was laid out.
#[derive(Clone, Debug, Default)]
pub(crate) struct Measured {
    pub size: Option<(f32, f32)>,
    pub handles: Vec<HandleBounds>,
}

/// Where the minimap drew the flow, so a press in it can be read back as a flow point.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MinimapMap {
    pub bounds: Bounds<Pixels>,
    /// The flow point at the top left of `bounds`.
    pub origin: XY,
    /// Pixels per flow unit.
    pub scale: f32,
}

/// What the flow's elements tell the flow while they're laid out and painted, and what its
/// event handlers read back.
#[derive(Default)]
pub(crate) struct Shared {
    /// The pane's bounds in the window.
    pub pane: Cell<Bounds<Pixels>>,
    pub viewport: Cell<Viewport>,
    pub measured: RefCell<HashMap<NodeId, Measured>>,
    /// Each edge's line as it was last painted, bottom first.
    pub edges: RefCell<Vec<EdgeLine>>,
    pub minimap: Cell<Option<MinimapMap>>,
    /// What had the focus when the mouse button last went down, before anything handled the
    /// press: a node's child that takes the focus for that press owns it.
    pub press_focus: RefCell<Option<Option<FocusHandle>>>,
    /// The canvas transform last handed to the host's listener.
    pub transform: Cell<Option<crate::flow::CanvasTransform>>,
    /// Whether the last input was a touch rather than the mouse: presses near edges reach
    /// further for a finger.
    pub touch: Cell<bool>,
    /// Whether the last frame drew an animated edge, and whether the one before did.
    pub animating: Cell<bool>,
    pub was_animating: Cell<bool>,
}

impl Shared {
    /// A window point as a point in the pane.
    pub fn pane_point(&self, p: Point<Pixels>) -> XY {
        let o = self.pane.get().origin;
        xy(f32::from(p.x - o.x), f32::from(p.y - o.y))
    }

    /// A rectangle in the window as one in the flow.
    fn flow_rect(&self, b: Bounds<Pixels>) -> Rect {
        let v = self.viewport.get();
        let a = v.to_flow(self.pane_point(b.origin));
        Rect::new(
            a.x,
            a.y,
            f32::from(b.size.width) / v.zoom,
            f32::from(b.size.height) / v.zoom,
        )
    }
}

/// What a node's renderer is given besides the node: what state it's in, the flow's colors, and
/// the pieces only the flow can make, its handles and its resizer.
#[derive(Clone)]
pub struct NodeContext {
    pub(crate) flow: WeakEntity<Flow>,
    pub(crate) shared: Rc<Shared>,
    pub(crate) id: NodeId,
    pub(crate) origin: XY,
    /// Whether the node is selected.
    pub selected: bool,
    /// Whether handles on this node take connections.
    pub connectable: bool,
    pub zoom: f32,
    pub colors: FlowColors,
    /// How large (in points) the area a finger can take a handle or a grip by is, at least.
    pub(crate) touch_size: Pixels,
}

impl NodeContext {
    /// The flow this node is in, for event handlers to update later. While the node is being
    /// drawn the flow is busy: reading or updating it then panics. Read the `&Flow` the
    /// renderer is given instead.
    pub fn flow(&self) -> &WeakEntity<Flow> {
        &self.flow
    }

    /// The size for text of `n` flow units in this node: use it rather than [`units`] for
    /// `text_size`, so zooming stays cheap (see [`font_size`]).
    pub fn font_size(&self, n: f32) -> Pixels {
        font_size(n, self.zoom)
    }

    /// A handle on one side of the node. Add it as a child of the node's outermost box, which
    /// it positions itself against; that box mustn't clip its overflow.
    pub fn handle(&self, kind: HandleKind, position: Side) -> Handle {
        Handle {
            ctx: self.clone(),
            kind,
            position,
            id: None,
            at: 0.5,
            connectable: self.connectable,
            color: None,
            border: px(1.),
        }
    }

    /// ResizeCorner grips that resize the node while it's selected. Add it as a child of the node's
    /// outermost box, and let that box fill the node (`size_full`) so it follows the new size.
    pub fn resizer(&self, min_width: f32, min_height: f32) -> impl IntoElement + use<> {
        let grip = units(8.);
        let out = units(-4.);
        let color = self.colors.selected;
        let corner = |corner: ResizeCorner| {
            let flow = self.flow.clone();
            let id = self.id.clone();
            let base = div().absolute().size(grip).bg(color).rounded(units(1.));
            let placed = match corner {
                ResizeCorner::TopLeft => base
                    .top(out)
                    .left(out)
                    .cursor(CursorStyle::ResizeUpLeftDownRight),
                ResizeCorner::TopRight => base
                    .top(out)
                    .right(out)
                    .cursor(CursorStyle::ResizeUpRightDownLeft),
                ResizeCorner::BottomLeft => base
                    .bottom(out)
                    .left(out)
                    .cursor(CursorStyle::ResizeUpRightDownLeft),
                ResizeCorner::BottomRight => base
                    .bottom(out)
                    .right(out)
                    .cursor(CursorStyle::ResizeUpLeftDownRight),
            };
            let touch = touch_target(
                flow.clone(),
                TouchTarget::Resize {
                    node: id.clone(),
                    corner,
                    min: (min_width, min_height),
                },
                self.touch_size,
                Reach::Corner(corner),
            );
            placed
                .child(touch)
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    let took = flow
                        .update(cx, |flow, cx| {
                            flow.resize_mouse_down(
                                &id,
                                corner,
                                (min_width, min_height),
                                event,
                                window,
                                cx,
                            )
                        })
                        .unwrap_or(false);
                    if took {
                        cx.stop_propagation();
                    }
                })
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .when(self.selected, |d| {
                d.border_1()
                    .border_color(color)
                    .child(corner(ResizeCorner::TopLeft))
                    .child(corner(ResizeCorner::TopRight))
                    .child(corner(ResizeCorner::BottomLeft))
                    .child(corner(ResizeCorner::BottomRight))
            })
    }
}

/// A connection point on a node's side. Dragging from one to another makes an edge.
#[derive(IntoElement)]
pub struct Handle {
    ctx: NodeContext,
    kind: HandleKind,
    position: Side,
    id: Option<SharedString>,
    at: f32,
    connectable: bool,
    color: Option<Hsla>,
    border: Pixels,
}

impl Handle {
    /// Names the handle, so edges can say which of several they use.
    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Where along its side the handle sits, from 0 (the start) to 1; half way by default.
    pub fn at(mut self, fraction: f32) -> Self {
        self.at = fraction;
        self
    }

    /// Whether connections can start or end here.
    pub fn connectable(mut self, on: bool) -> Self {
        self.connectable = on;
        self
    }

    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// How wide the border of the box the handle is on is, so the handle can center itself on
    /// the box's outer edge; one pixel unless said.
    pub fn border_width(mut self, width: Pixels) -> Self {
        self.border = width;
        self
    }
}

impl RenderOnce for Handle {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Handle {
            ctx,
            kind,
            position,
            id,
            at,
            connectable,
            color,
            border,
        } = self;
        let out = units(-HANDLE_SIZE / 2.);
        let base = div()
            .absolute()
            .size(units(HANDLE_SIZE))
            .rounded_full()
            .bg(color.unwrap_or(ctx.colors.handle))
            .border_1()
            .border_color(ctx.colors.handle_border);
        // Offsets count from inside the box's border, so the dot goes out by that much more
        // to sit centered on the box's outer edge.
        let b = -border;
        let placed = match position {
            Side::Top => base.top(out).mt(b).left(relative(at)).ml(out),
            Side::Bottom => base.bottom(out).mb(b).left(relative(at)).ml(out),
            Side::Left => base.left(out).ml(b).top(relative(at)).mt(out),
            Side::Right => base.right(out).mr(b).top(relative(at)).mt(out),
        };

        // Tells the flow where the handle ended up, for the edges and for connecting.
        let measure = {
            let shared = ctx.shared.clone();
            let node = ctx.id.clone();
            let origin = ctx.origin;
            let id = id.clone();
            canvas(
                move |bounds, _, _| {
                    // The canvas fills the dot inside its one-pixel border; the dot is that
                    // much larger all round.
                    let mut rect = shared.flow_rect(bounds.dilate(px(1.)));
                    rect.x -= origin.x;
                    rect.y -= origin.y;
                    shared
                        .measured
                        .borrow_mut()
                        .entry(node)
                        .or_default()
                        .handles
                        .push(HandleBounds {
                            id,
                            kind,
                            position,
                            rect,
                            connectable,
                        });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };

        let touch = touch_target(
            ctx.flow.clone(),
            TouchTarget::Handle {
                node: ctx.id.clone(),
                handle: id.clone(),
                kind,
            },
            ctx.touch_size,
            Reach::Out(position),
        );
        placed
            .when(connectable, |d| d.child(touch))
            .when(connectable, |d| {
                let flow = ctx.flow.clone();
                let node = ctx.id.clone();
                d.cursor(CursorStyle::Crosshair).on_mouse_down(
                    MouseButton::Left,
                    move |event, window, cx| {
                        let took = flow
                            .update(cx, |flow, cx| {
                                flow.handle_mouse_down(&node, id.clone(), kind, event, window, cx)
                            })
                            .unwrap_or(false);
                        if took {
                            cx.stop_propagation();
                        }
                    },
                )
            })
            .child(measure)
    }
}

/// Tells the flow how large a node came out, and clears its handles for them to report again.
pub(crate) fn measure_node(shared: Rc<Shared>, flow: EntityId, id: NodeId) -> impl IntoElement {
    canvas(
        move |bounds, window, cx| {
            let zoom = shared.viewport.get().zoom;
            let size = (
                f32::from(bounds.size.width) / zoom,
                f32::from(bounds.size.height) / zoom,
            );
            let mut measured = shared.measured.borrow_mut();
            let entry = measured.entry(id).or_default();
            // A point of slack: text rounds to whole pixels differently at each zoom.
            let slack = (1. / zoom).max(0.5);
            let changed = entry
                .size
                .is_none_or(|old| (old.0 - size.0).abs() > slack || (old.1 - size.1).abs() > slack);
            entry.size = Some(size);
            entry.handles.clear();
            if changed {
                // What the flow decided from the old size (what's on screen, a pending fit) is
                // decided again on the next frame.
                cx.notify(flow);
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Lays out and paints its child with a rem of the given size, which is what makes a node
/// built in [`units`] follow the zoom.
pub(crate) struct Zoomed {
    rem: Pixels,
    child: AnyElement,
}

impl Zoomed {
    pub fn new(zoom: f32, child: impl IntoElement) -> Self {
        Self {
            rem: px(16. * zoom),
            child: child.into_any_element(),
        }
    }
}

impl IntoElement for Zoomed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Zoomed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let id = window.with_rem_size(Some(self.rem), |window| {
            self.child.request_layout(window, cx)
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(self.rem), |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(self.rem), |window| self.child.paint(window, cx));
    }
}

/// The box the built-in `default`, `input` and `output` kinds share: the label, centered, with
/// a target handle, a source handle, or both.
pub fn labeled_node(node: &Node, ctx: &NodeContext, target: bool, source: bool) -> AnyElement {
    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .text_center()
        .p(units(10.))
        .rounded(units(3.))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            ctx.colors.node_border
        })
        .bg(ctx.colors.node)
        .map(|d| match node.width {
            Some(_) => d.w_full(),
            None => d.w(units(150.)),
        })
        .when(node.height.is_some(), |d| d.h_full())
        .child(node.label_text())
        .when(target, |d| {
            d.child(ctx.handle(HandleKind::Target, node.target_position))
        })
        .when(source, |d| {
            d.child(ctx.handle(HandleKind::Source, node.source_position))
        })
        .into_any_element()
}

/// The built-in `group` kind: a plain area other nodes can be children of.
pub fn group_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    div()
        .w(units(node.width.unwrap_or(200.)))
        .h(units(node.height.unwrap_or(150.)))
        .rounded(units(3.))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            ctx.colors.node_border
        })
        .bg(ctx.colors.group)
        .p(units(6.))
        .text_size(ctx.font_size(10.))
        .text_color(ctx.colors.muted_text)
        .child(node.label_text())
        .into_any_element()
}

/// The renderers every flow starts with.
pub(crate) fn built_in() -> HashMap<SharedString, NodeRenderer> {
    let mut kinds: HashMap<SharedString, NodeRenderer> = HashMap::new();
    kinds.insert(
        "default".into(),
        Rc::new(|n, c, _, _, _| labeled_node(n, c, true, true)),
    );
    kinds.insert(
        "input".into(),
        Rc::new(|n, c, _, _, _| labeled_node(n, c, false, true)),
    );
    kinds.insert(
        "output".into(),
        Rc::new(|n, c, _, _, _| labeled_node(n, c, true, false)),
    );
    kinds.insert("group".into(), Rc::new(|n, c, _, _, _| group_node(n, c)));
    kinds
}
