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
use serde_json::Value;

use crate::flow::Flow;
use crate::shapes::geometry;
use crate::theme::FlowColors;
use crate::types::{
    HandleKind, LabelAlign, Node, NodeId, Rect, ResizeCorner, Side, Viewport, XY, xy,
};

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
        /// Whether the node keeps its proportions.
        keep: bool,
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
    /// A handle that isn't drawn: edges meet its middle, which is on the node's outline, rather
    /// than the outer side of a dot nobody sees.
    pub hidden: bool,
}

impl HandleBounds {
    /// Where an edge meets the handle: the middle of its outer side, or its middle when it
    /// isn't drawn.
    pub fn anchor(&self) -> XY {
        let c = self.rect.center();
        if self.hidden {
            return c;
        }
        match self.position {
            Side::Left => xy(self.rect.x, c.y),
            Side::Right => xy(self.rect.right(), c.y),
            Side::Top => xy(c.x, self.rect.y),
            Side::Bottom => xy(c.x, self.rect.bottom()),
        }
    }
}

/// A routed edge's route (none: it takes its usual one), with a hash of what it was worked out
/// from.
pub(crate) type Route = (u64, Option<Vec<XY>>);

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
    /// Where its label is centred, in pane coordinates, if it has one.
    pub label: Option<XY>,
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
/// A map looked up for every node on every frame: hashed fast, and seeded at random so no
/// document can pick ids that collide.
pub(crate) type FastMap<K, V> = HashMap<K, V, foldhash::fast::RandomState>;

#[derive(Default)]
pub(crate) struct Shared {
    /// The pane's bounds in the window.
    pub pane: Cell<Bounds<Pixels>>,
    pub viewport: Cell<Viewport>,
    pub measured: RefCell<FastMap<NodeId, Measured>>,
    /// Each edge's line as it was last painted, bottom first.
    pub edges: RefCell<Vec<EdgeLine>>,
    pub minimap: Cell<Option<MinimapMap>>,
    /// What had the focus when the mouse button last went down, before anything handled the
    /// press: a node's child that takes the focus for that press owns it.
    pub press_focus: RefCell<Option<Option<FocusHandle>>>,
    /// Each routed edge's route, with what it was worked out from, so it's worked out again
    /// only when its ends or a node near it move.
    pub routes: RefCell<HashMap<SharedString, Route>>,
    /// The canvas transform last handed to the host's listener.
    pub transform: Cell<Option<crate::flow::CanvasTransform>>,
    /// Whether the last input was a touch rather than the mouse: presses near edges reach
    /// further for a finger.
    pub touch: Cell<bool>,
    /// Whether the last frame drew an animated edge, and whether the one before did.
    pub animating: Cell<bool>,
    pub was_animating: Cell<bool>,
    /// Each ink node's stroke, read once for as long as its stored form stays the same.
    pub ink: RefCell<HashMap<NodeId, InkCache>>,
    /// The window's size when last drawn: what the pane is taken to be before its first
    /// layout, so even the first frame draws only what's in view.
    pub window: Cell<(f32, f32)>,
    /// How many nodes out of view have yet to be drawn for the first time.
    pub unmeasured: Cell<usize>,
}

/// An ink node's stroke as read, and what it was read from: the points by where their text is
/// and a little of it, the rest as it was.
pub(crate) struct InkCache {
    points: (usize, usize, [u8; 8], [u8; 8]),
    rest: [Value; 4],
    pub drawn: Rc<crate::ink::Drawn>,
}

impl Shared {
    /// An ink node's stroke ready to draw; none when its `data.ink` doesn't read as one.
    pub fn ink(&self, node: &Node) -> Option<Rc<crate::ink::Drawn>> {
        let ink = node.data.get("ink")?;
        let text = ink.get("points")?.as_str()?;
        let b = text.as_bytes();
        let ends = |s: &[u8]| {
            let mut a = [0u8; 8];
            let n = s.len().min(8);
            a[..n].copy_from_slice(&s[..n]);
            a
        };
        let points = (
            text.as_ptr() as usize,
            b.len(),
            ends(b),
            ends(&b[b.len().saturating_sub(8)..]),
        );
        let field = |k: &str| ink.get(k).cloned().unwrap_or(Value::Null);
        let rest = [
            field("tool"),
            field("color"),
            field("width"),
            field("transform"),
        ];
        let mut cache = self.ink.borrow_mut();
        if let Some(c) = cache.get(&node.id)
            && c.points == points
            && c.rest == rest
        {
            return Some(c.drawn.clone());
        }
        let stroke = crate::ink::InkStroke::of(node)?;
        let drawn = Rc::new(crate::ink::Drawn::of(&stroke));
        cache.insert(
            node.id.clone(),
            InkCache {
                points,
                rest,
                drawn: drawn.clone(),
            },
        );
        Some(drawn)
    }
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
    /// Whether finger-sized targets are built (see [`crate::FlowOptions::touch_targets`]).
    pub(crate) touch_targets: bool,
    /// In a tree layout, how deep the node is (0 for a root); none otherwise.
    pub depth: Option<usize>,
    /// In a tree layout, how many children the node has, and whether they're folded away.
    pub children: usize,
    pub collapsed: bool,
    /// What the node links to, if anything.
    pub link: Option<SharedString>,
    pub(crate) images: Option<ImageLoader>,
}

/// Turns a picture's `src` into something GPUI can draw, as the host fetches or reads it. The
/// flow only ever asks this: it fetches nothing itself.
pub type ImageLoader = Rc<dyn Fn(&str, &mut Window, &mut App) -> Option<ImageSource>>;

impl NodeContext {
    /// The flow this node is in, for event handlers to update later. While the node is being
    /// drawn the flow is busy: reading or updating it then panics. Read the `&Flow` the
    /// renderer is given instead.
    pub fn flow(&self) -> &WeakEntity<Flow> {
        &self.flow
    }

    /// A small button that asks the host to open the node's link (a `FlowEvent::OpenLink`),
    /// or nothing when the node has none. The built-in kinds show it after their label; add
    /// it to a kind of your own wherever it fits.
    pub fn link_button(&self) -> Option<AnyElement> {
        self.link.as_ref()?;
        let (flow, id) = (self.flow.clone(), self.id.clone());
        Some(
            div()
                .id(SharedString::from(format!("link-{}", self.id)))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(units(18.))
                .rounded(units(4.))
                .text_size(self.font_size(11.))
                .text_color(self.colors.selected)
                .cursor(CursorStyle::PointingHand)
                .hover(|s| s.bg(self.colors.panel_hover))
                .nodrag()
                .child("↗")
                .on_click(move |_, _, cx| {
                    flow.update(cx, |flow, cx| flow.open_link(&id, cx)).ok();
                })
                .into_any_element(),
        )
    }

    /// The picture the host's loader gives for `src`, or none: no loader, or nothing for it.
    pub fn image(&self, src: &str, window: &mut Window, cx: &mut App) -> Option<ImageSource> {
        if src.trim().is_empty() {
            return None;
        }
        self.images.as_ref().and_then(|load| load(src, window, cx))
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
            inset: 0.,
            hidden: false,
        }
    }

    /// ResizeCorner grips that resize the node while it's selected. Add it as a child of the node's
    /// outermost box, and let that box fill the node (`size_full`) so it follows the new size.
    pub fn resizer(&self, min_width: f32, min_height: f32) -> impl IntoElement + use<> {
        self.resizer_with(min_width, min_height, false)
    }

    /// Like [`NodeContext::resizer`], but the node keeps the proportions it has: a picture's.
    pub fn resizer_keeping_proportions(
        &self,
        min_width: f32,
        min_height: f32,
    ) -> impl IntoElement + use<> {
        self.resizer_with(min_width, min_height, true)
    }

    fn resizer_with(
        &self,
        min_width: f32,
        min_height: f32,
        keep: bool,
    ) -> impl IntoElement + use<> {
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
                    keep,
                },
                self.touch_size,
                Reach::Corner(corner),
            );
            placed
                .when(self.touch_targets, |d| d.child(touch))
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    let took = flow
                        .update(cx, |flow, cx| {
                            flow.resize_mouse_down(
                                &id,
                                corner,
                                (min_width, min_height),
                                keep,
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
    inset: f32,
    hidden: bool,
}

impl Handle {
    /// How far (in flow units) in from the box's side the handle sits: onto a shape's visible
    /// edge, such as a parallelogram's slanted side.
    pub fn inset(mut self, inset: f32) -> Self {
        self.inset = inset;
        self
    }

    /// Takes connections and has edges meet it as ever, but isn't drawn: a whiteboard shape's
    /// handles show only while it's selected.
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

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
            inset,
            hidden,
        } = self;
        let out = units(-HANDLE_SIZE / 2. + inset);
        let along = units(-HANDLE_SIZE / 2.);
        let (fill, rim) = if hidden {
            (gpui::transparent_black(), gpui::transparent_black())
        } else {
            (color.unwrap_or(ctx.colors.handle), ctx.colors.handle_border)
        };
        let base = div()
            .absolute()
            .size(units(HANDLE_SIZE))
            .rounded_full()
            .bg(fill)
            .border_1()
            .border_color(rim);
        // Offsets count from inside the box's border, so the dot goes out by that much more
        // to sit centered on the box's outer edge.
        let b = -border;
        let placed = match position {
            Side::Top => base.top(out).mt(b).left(relative(at)).ml(along),
            Side::Bottom => base.bottom(out).mb(b).left(relative(at)).ml(along),
            Side::Left => base.left(out).ml(b).top(relative(at)).mt(along),
            Side::Right => base.right(out).mr(b).top(relative(at)).mt(along),
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
                            hidden,
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
            // A finger takes a handle only while it's drawn: one nobody can see mustn't turn
            // a touch at a shape's edge into a connection.
            .when(connectable && !hidden && ctx.touch_targets, |d| {
                d.child(touch)
            })
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
    let pencil = ctx.colors.pencil(node.color.as_deref());
    let align = node.text_align.unwrap_or_default();
    div()
        .relative()
        .flex()
        .items_center()
        .map(|d| match align {
            LabelAlign::Left => d.justify_start(),
            LabelAlign::Center => d.justify_center(),
            LabelAlign::Right => d.justify_end(),
        })
        .p(units(10.))
        .rounded(units(3.))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            pencil.stroke
        })
        .bg(pencil.fill)
        .text_color(pencil.text)
        .children(badges(node, ctx, pencil.text))
        .map(|d| match node.width {
            Some(_) => d.w_full(),
            None => d.w(units(150.)),
        })
        .when(node.height.is_some(), |d| d.h_full())
        .gap(units(6.))
        .child(label(node, align))
        .children(ctx.link_button())
        .when(target, |d| {
            d.child(ctx.handle(HandleKind::Target, node.target_position))
        })
        .when(source, |d| {
            d.child(ctx.handle(HandleKind::Source, node.source_position))
        })
        .into_any_element()
}

/// A node's label as the built-in kinds show it: wrapped to the box (a word too long for it
/// breaks), and cut off where a box of a fixed height ends.
fn label(node: &Node, align: LabelAlign) -> Div {
    div()
        .min_w_0()
        .max_h_full()
        .overflow_hidden()
        .map(|d| match align {
            LabelAlign::Left => d.text_left(),
            LabelAlign::Center => d.text_center(),
            LabelAlign::Right => d.text_right(),
        })
        .child(node.label_text())
}

/// A node's icon (one gpui-core bundles) and emoji, side by side, or nothing when it has
/// neither.
pub(crate) fn badges(node: &Node, ctx: &NodeContext, color: Hsla) -> Option<Div> {
    let icon = node
        .icon
        .as_deref()
        .and_then(gpui_core::IconName::named)
        .map(|name| gpui_core::Icon::new(name).size(units(14.)).color(color));
    // One emoji: a short run of characters, as a single one may be several code points.
    let emoji = node
        .emoji
        .as_ref()
        .map(|e| e.trim())
        .filter(|e| !e.is_empty() && e.chars().count() <= 12)
        .map(|e| SharedString::from(e.to_string()));
    if icon.is_none() && emoji.is_none() {
        return None;
    }
    Some(
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(units(3.))
            .text_size(ctx.font_size(13.))
            .children(icon)
            .children(emoji),
    )
}

/// The built-in `shape` kind, for whiteboards: the node's outline ([`NodeShape`](crate::shapes::NodeShape)) in its
/// palette color, its label inside the shape's text area, its badges near its top right, and a
/// handle each way on every side, drawn while it's selected.
pub fn shape_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let shape = node.shape.unwrap_or_default();
    let (dw, dh) = shape.default_size();
    let (w, h) = (node.width.unwrap_or(dw), node.height.unwrap_or(dh));
    let g = geometry(shape, w, h);
    let pencil = ctx.colors.pencil(node.color.as_deref());
    let stroke = if ctx.selected {
        ctx.colors.selected
    } else {
        pencil.stroke
    };
    let line = if ctx.selected { 2. } else { 1. };
    let (outline, detail) = (g.outline.clone(), g.detail.clone());
    let fill = pencil.fill;
    let painted = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            crate::paint::paint_shape(&outline, &detail, bounds, w, fill, stroke, line, window)
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    let t = g.text;
    let align = node.text_align.unwrap_or_default();
    let text = div()
        .absolute()
        .left(units(t.x))
        .top(units(t.y))
        .w(units(t.w))
        .h(units(t.h))
        .flex()
        .items_center()
        .map(|d| match align {
            LabelAlign::Left => d.justify_start(),
            LabelAlign::Center => d.justify_center(),
            LabelAlign::Right => d.justify_end(),
        })
        .gap(units(6.))
        .text_color(pencil.text)
        .child(label(node, align))
        .children(ctx.link_button());
    // Ending at the shape's badge point and growing back from it, so an icon and an emoji
    // both stay inside the box, and a diamond's or a cloud's sit near its outline.
    let badge = badges(node, ctx, pencil.text).map(|b| {
        div()
            .absolute()
            .right(units((w - g.badge.x - 9.).max(2.)))
            .top(units((g.badge.y - 9.).max(2.)))
            .child(b)
    });
    let hidden = !ctx.selected;
    let handles = [Side::Top, Side::Right, Side::Bottom, Side::Left]
        .into_iter()
        .flat_map(|side| {
            let a = g.anchors.on(side);
            let (at, inset) = match side {
                Side::Top => (a.x / w, a.y),
                Side::Bottom => (a.x / w, h - a.y),
                Side::Left => (a.y / h, a.x),
                Side::Right => (a.y / h, w - a.x),
            };
            let name: SharedString = match side {
                Side::Top => "top",
                Side::Right => "right",
                Side::Bottom => "bottom",
                Side::Left => "left",
            }
            .into();
            // Both kinds, so edges meet any side whichever way they run; the source on top,
            // so a drag starts a connection out.
            [HandleKind::Target, HandleKind::Source].map(|kind| {
                ctx.handle(kind, side)
                    .id(name.clone())
                    .at(at)
                    .inset(inset)
                    .border_width(px(0.))
                    .hidden(hidden)
            })
        });
    div()
        .relative()
        .w(units(w))
        .h(units(h))
        .text_size(ctx.font_size(13.))
        .child(painted)
        .child(text)
        .children(badge)
        .children(handles)
        .into_any_element()
}

/// The built-in `group` kind: a plain area other nodes can be children of.
pub fn group_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    div()
        .overflow_hidden()
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

/// How tall a frame's title band is, and how wide the band along its border that takes presses
/// (inside it, a press is the pane's), in flow units.
pub(crate) const FRAME_TITLE: f32 = 28.;
pub(crate) const FRAME_BORDER: f32 = 10.;

/// The built-in `frame` kind: an area that holds other nodes (their parent), with its title
/// over it. It's taken by its title and its border, and resized by its corners while selected.
pub fn frame_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let pencil = ctx.colors.pencil(node.color.as_deref());
    let colored = node.color.is_some();
    let (w, h) = (node.width.unwrap_or(360.), node.height.unwrap_or(240.));
    let stroke = if ctx.selected {
        ctx.colors.selected
    } else if colored {
        pencil.stroke
    } else {
        ctx.colors.node_border
    };
    let title = div()
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(units(FRAME_TITLE))
        .px(units(4.))
        .flex()
        .items_center()
        .gap(units(6.))
        .overflow_hidden()
        .text_size(ctx.font_size(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if colored {
            pencil.text
        } else {
            ctx.colors.muted_text
        })
        .children(badges(node, ctx, pencil.text))
        .child(
            div()
                .min_w_0()
                .max_h_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(node.label_text()),
        );
    let body = div()
        .absolute()
        .left_0()
        .top(units(FRAME_TITLE))
        .w_full()
        .h(units((h - FRAME_TITLE).max(0.)))
        .rounded(units(8.))
        .border_1()
        .border_color(stroke)
        .bg(if colored {
            pencil.fill.opacity(0.55)
        } else {
            ctx.colors.group
        })
        .child(ctx.resizer(120., FRAME_TITLE + 40.));
    div()
        .relative()
        .w(units(w))
        .h(units(h))
        .child(title)
        .child(body)
        .into_any_element()
}

/// The built-in `topic` kind, for mind maps: the label in a rounded box with no handles (a tree
/// layout joins topics by their sides), larger and bolder for a root.
pub fn topic_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let root = ctx.depth == Some(0);
    let pencil = ctx.colors.pencil(node.color.as_deref());
    let align = node.text_align.unwrap_or(LabelAlign::Left);
    div()
        .relative()
        .max_w(units(260.))
        .px(units(if root { 16. } else { 12. }))
        .py(units(if root { 10. } else { 7. }))
        .rounded(units(if root { 10. } else { 7. }))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            pencil.stroke
        })
        .bg(pencil.fill)
        .text_color(pencil.text)
        .text_size(ctx.font_size(if root { 16. } else { 13. }))
        .when(root, |d| d.font_weight(FontWeight::SEMIBOLD))
        .flex()
        .items_center()
        .gap(units(6.))
        .children(badges(node, ctx, pencil.text))
        .child(label(node, align))
        .children(ctx.link_button())
        .into_any_element()
}

/// How tall an `image` node's caption is, in flow units.
const CAPTION: f32 = 26.;

/// The built-in `image` kind: the picture the host's loader gives for its `media.src`, scaled
/// to fit, its caption (the label) under it, and its alt text in its place until it loads (or
/// when there's no loader). It keeps its proportions when resized.
pub fn image_node(node: &Node, ctx: &NodeContext, window: &mut Window, cx: &mut App) -> AnyElement {
    let media = node.media.clone().unwrap_or_default();
    let caption = node.label_text();
    let strip = if caption.is_empty() { 0. } else { CAPTION };
    // The picture's own proportions, at most 320 wide, unless the node has a size.
    let (pw, ph) = match (media.width, media.height) {
        (Some(w), Some(h)) if w > 0. && h > 0. => {
            let k = (320. / w).min(1.);
            (w * k, h * k)
        }
        _ => (320., 240.),
    };
    let (w, h) = (node.width.unwrap_or(pw), node.height.unwrap_or(ph + strip));
    let placeholder = {
        let (alt, muted, panel) = (media.alt.clone(), ctx.colors.muted_text, ctx.colors.group);
        let font = ctx.font_size(11.);
        move || {
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(units(6.))
                .p(units(10.))
                .bg(panel)
                .text_size(font)
                .text_color(muted)
                .overflow_hidden()
                .child(
                    gpui_core::Icon::new(gpui_core::IconName::named("image").unwrap_or_else(
                        || {
                            gpui_core::IconName::all()
                                .next()
                                .expect("gpui-core bundles icons")
                        },
                    ))
                    .size(units(20.))
                    .color(muted),
                )
                .child(
                    div()
                        .min_w_0()
                        .max_h_full()
                        .overflow_hidden()
                        .text_center()
                        .child(alt.clone()),
                )
                .into_any_element()
        }
    };
    let picture = match ctx.image(&media.src, window, cx) {
        Some(source) => img(source)
            .size_full()
            .object_fit(ObjectFit::Contain)
            .with_fallback(placeholder.clone())
            .with_loading(placeholder.clone())
            .into_any_element(),
        None => placeholder(),
    };
    div()
        .relative()
        .w(units(w))
        .h(units(h))
        .flex()
        .flex_col()
        .rounded(units(6.))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            ctx.colors.node_border
        })
        .bg(ctx.colors.node)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .rounded_t(units(6.))
                .overflow_hidden()
                .child(picture),
        )
        .when(strip > 0., |d| {
            d.child(
                div()
                    .flex_none()
                    .h(units(CAPTION))
                    .px(units(8.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_size(ctx.font_size(11.))
                    .text_color(ctx.colors.muted_text)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(caption),
                    ),
            )
        })
        .child(ctx.resizer_keeping_proportions(60., 40.))
        .into_any_element()
}

/// The built-in `link` kind: a card for an address, showing what the host filled in (its
/// site, title, description and preview picture; the address alone until then), and a button
/// that asks the host to open it (`FlowEvent::OpenUrl`). The flow never opens or fetches it.
pub fn link_node(node: &Node, ctx: &NodeContext, window: &mut Window, cx: &mut App) -> AnyElement {
    let card = node.card.clone().unwrap_or_default();
    let w = node.width.unwrap_or(280.);
    let muted = ctx.colors.muted_text;
    let preview = card
        .image
        .as_deref()
        .and_then(|src| ctx.image(src, window, cx))
        .map(|source| {
            div()
                .flex_none()
                .h(units(120.))
                .overflow_hidden()
                .rounded_t(units(6.))
                .child(img(source).size_full().object_fit(ObjectFit::Cover))
        });
    let line = |text: SharedString, size: f32, lines: usize| {
        div()
            .min_w_0()
            .overflow_hidden()
            .text_size(ctx.font_size(size))
            .line_clamp(lines)
            .child(text)
    };
    let open = {
        let (flow, id) = (ctx.flow.clone(), ctx.id.clone());
        div()
            .id(SharedString::from(format!("open-{}", ctx.id)))
            .flex_none()
            .px(units(8.))
            .py(units(3.))
            .rounded(units(4.))
            .border_1()
            .border_color(ctx.colors.node_border)
            .text_size(ctx.font_size(10.))
            .text_color(muted)
            .cursor(CursorStyle::PointingHand)
            .hover(|s| s.bg(ctx.colors.panel_hover))
            .nodrag()
            .child("Open ↗")
            .on_click(move |_, _, cx| {
                flow.update(cx, |flow, cx| flow.open_url(&id, cx)).ok();
            })
    };
    div()
        .relative()
        .w(units(w))
        .when_some(node.height, |d, h| d.h(units(h)).overflow_hidden())
        .flex()
        .flex_col()
        .rounded(units(6.))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            ctx.colors.node_border
        })
        .bg(ctx.colors.node)
        .children(preview)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(units(4.))
                .px(units(12.))
                .py(units(10.))
                .min_w_0()
                .children(card.site.map(|s| line(s, 10., 1).text_color(muted)))
                .child(
                    line(card.title.unwrap_or_else(|| card.url.clone()), 13., 2)
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .children(card.description.map(|d| line(d, 11., 3).text_color(muted)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(units(8.))
                        .min_w_0()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(ctx.font_size(10.))
                                .text_color(muted)
                                .child(card.url.clone()),
                        )
                        .child(open),
                ),
        )
        .child(
            ctx.handle(HandleKind::Target, Side::Left)
                .hidden(!ctx.selected),
        )
        .child(
            ctx.handle(HandleKind::Source, Side::Right)
                .hidden(!ctx.selected),
        )
        .into_any_element()
}

/// The built-in `text` kind: words on the canvas with no box round them, as a heading, in the
/// node's palette color. Selected, a faint outline shows where it is.
pub fn text_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let pencil = ctx.colors.pencil(node.color.as_deref());
    let color = if node.color.is_some() {
        pencil.text
    } else {
        ctx.colors.node_text
    };
    let align = node.text_align.unwrap_or(LabelAlign::Left);
    div()
        .relative()
        .w(units(node.width.unwrap_or(240.)))
        .when_some(node.height, |d, h| d.h(units(h)))
        .p(units(4.))
        .rounded(units(4.))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            gpui::transparent_black()
        })
        .text_size(ctx.font_size(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .flex()
        .items_center()
        .gap(units(6.))
        .children(badges(node, ctx, color))
        .child(label(node, align))
        .into_any_element()
}

/// The built-in `drawing` kind: a line drawn with a pen, as points across the node's box (its
/// `data.points`, "x,y x,y …" each from 0 to 1, as subcanvas keeps them), so it scales with the
/// node.
pub fn drawing_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let points: Vec<XY> = node
        .data
        .get("points")
        .and_then(|p| p.as_str())
        .map(parse_points)
        .unwrap_or_default();
    let (w, h) = (node.width.unwrap_or(40.), node.height.unwrap_or(40.));
    let color = if ctx.selected {
        ctx.colors.selected
    } else if node.color.is_some() {
        ctx.colors.pencil(node.color.as_deref()).stroke
    } else {
        ctx.colors.node_text
    };
    let zoom = ctx.zoom;
    div()
        .relative()
        .w(units(w))
        .h(units(h))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let line: Vec<XY> = points.iter().map(|p| xy(p.x * bw, p.y * bh)).collect();
                    crate::paint::stroke(bounds.origin, &line, (3. * zoom).max(1.), color, window);
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .into_any_element()
}

/// The built-in `ink` kind: a stroke drawn with a pen, stored as the iPad's Notes stores
/// PencilKit's (see [`crate::ink`]). It's drawn in its box, which is the stroke's bounds; one
/// that doesn't read draws nothing.
pub fn ink_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let Some(drawn) = ctx.shared.ink(node) else {
        return div().into_any_element();
    };
    let (w, h) = (drawn.size.x.max(0.5), drawn.size.y.max(0.5));
    let color = crate::paint::ink_color(drawn.color, drawn.tool, &ctx.colors);
    let halo = ctx.selected.then_some(ctx.colors.selected);
    div()
        .relative()
        .w(units(w))
        .h(units(h))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let scale = f32::from(bounds.size.width) / w;
                    crate::paint::paint_ink(
                        &drawn.outline,
                        bounds.origin,
                        scale,
                        color,
                        halo,
                        window,
                    );
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .into_any_element()
}

/// Points as subcanvas keeps a drawing's: "x,y x,y …", each kept from 0 to 1. What isn't a pair
/// of numbers is skipped, and a line is cut short at 4000 points.
pub fn parse_points(text: &str) -> Vec<XY> {
    text.split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            let (x, y) = (x.parse::<f32>().ok()?, y.parse::<f32>().ok()?);
            (x.is_finite() && y.is_finite()).then(|| xy(x.clamp(0., 1.), y.clamp(0., 1.)))
        })
        .take(4000)
        .collect()
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
    kinds.insert("ink".into(), Rc::new(|n, c, _, _, _| ink_node(n, c)));
    kinds.insert("topic".into(), Rc::new(|n, c, _, _, _| topic_node(n, c)));
    kinds.insert("shape".into(), Rc::new(|n, c, _, _, _| shape_node(n, c)));
    kinds.insert("frame".into(), Rc::new(|n, c, _, _, _| frame_node(n, c)));
    kinds.insert("text".into(), Rc::new(|n, c, _, _, _| text_node(n, c)));
    kinds.insert(
        "drawing".into(),
        Rc::new(|n, c, _, _, _| drawing_node(n, c)),
    );
    kinds.insert(
        "image".into(),
        Rc::new(|n, c, _, w, cx| image_node(n, c, w, cx)),
    );
    kinds.insert(
        "link".into(),
        Rc::new(|n, c, _, w, cx| link_node(n, c, w, cx)),
    );
    kinds
}
