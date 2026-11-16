//! What a flow is made of: nodes, edges, the viewport, and the small types they share.
//!
//! Everything here is plain data in flow coordinates, where one unit is one point on screen at
//! zoom 1. It all saves and loads as JSON ([`FlowObject`]).

use std::ops::{Add, Mul, Sub};

use gpui::SharedString;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use crate::shapes::NodeShape;

/// A node's id. Unique among the flow's nodes.
pub type NodeId = SharedString;
/// An edge's id. Unique among the flow's edges.
pub type EdgeId = SharedString;

/// A point or a distance in flow coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct XY {
    pub x: f32,
    pub y: f32,
}

/// A point in flow coordinates.
pub const fn xy(x: f32, y: f32) -> XY {
    XY { x, y }
}

impl XY {
    pub fn distance(self, other: XY) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

impl Add for XY {
    type Output = XY;
    fn add(self, o: XY) -> XY {
        xy(self.x + o.x, self.y + o.y)
    }
}

impl Sub for XY {
    type Output = XY;
    fn sub(self, o: XY) -> XY {
        xy(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for XY {
    type Output = XY;
    fn mul(self, k: f32) -> XY {
        xy(self.x * k, self.y * k)
    }
}

/// A rectangle in flow coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// The rectangle with these two points as opposite corners, in either order.
    pub fn from_corners(a: XY, b: XY) -> Self {
        Self::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        )
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> XY {
        xy(self.x + self.w / 2., self.y + self.h / 2.)
    }

    pub fn contains(&self, p: XY) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// Whether the two overlap at all.
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    /// Whether `o` lies wholly inside.
    pub fn encloses(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, o: &Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect::new(
            x,
            y,
            self.right().max(o.right()) - x,
            self.bottom().max(o.bottom()) - y,
        )
    }
}

/// Where the flow sits in its pane: a flow point `p` is drawn at `p * zoom + (x, y)`, measured
/// from the pane's top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            zoom: 1.,
        }
    }
}

impl Viewport {
    pub const fn new(x: f32, y: f32, zoom: f32) -> Self {
        Self { x, y, zoom }
    }

    /// A point in the pane (from its top left corner) as a flow point.
    pub fn to_flow(&self, pane: XY) -> XY {
        xy((pane.x - self.x) / self.zoom, (pane.y - self.y) / self.zoom)
    }

    /// A flow point as a point in the pane.
    pub fn to_pane(&self, flow: XY) -> XY {
        xy(flow.x * self.zoom + self.x, flow.y * self.zoom + self.y)
    }

    /// The viewport at another zoom that keeps the flow point under `pane` where it is.
    pub fn zoomed_about(&self, pane: XY, zoom: f32) -> Viewport {
        let at = self.to_flow(pane);
        Viewport::new(pane.x - at.x * zoom, pane.y - at.y * zoom, zoom)
    }

    /// The part of the flow a pane of this size shows.
    pub fn visible(&self, width: f32, height: f32) -> Rect {
        let a = self.to_flow(xy(0., 0.));
        Rect::new(a.x, a.y, width / self.zoom, height / self.zoom)
    }
}

/// A side of a node: where a handle sits, and so which way its edge leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
        }
    }

    /// The unit vector pointing away from the node on this side.
    pub fn direction(self) -> XY {
        match self {
            Side::Left => xy(-1., 0.),
            Side::Right => xy(1., 0.),
            Side::Top => xy(0., -1.),
            Side::Bottom => xy(0., 1.),
        }
    }
}

/// A corner of a node, as its resizer's grips are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Which end of an edge a handle takes: edges run from a source handle to a target handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandleKind {
    Source,
    Target,
}

impl HandleKind {
    pub fn opposite(self) -> HandleKind {
        match self {
            HandleKind::Source => HandleKind::Target,
            HandleKind::Target => HandleKind::Source,
        }
    }
}

fn default_kind() -> SharedString {
    "default".into()
}

fn bottom() -> Side {
    Side::Bottom
}

fn top() -> Side {
    Side::Top
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_blank(s: &SharedString) -> bool {
    s.is_empty()
}

fn is_zero(n: &i32) -> bool {
    *n == 0
}

/// A node: a box on the canvas, drawn by the renderer registered for its `kind`.
///
/// The built-in kinds are `default` (a target handle and a source handle), `input` (a source
/// only), `output` (a target only) and `group` (a plain area for child nodes). They show
/// `data.label`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: NodeId,
    /// The top left corner: in the flow, or inside the parent when there is one.
    pub position: XY,
    #[serde(rename = "type", default = "default_kind")]
    pub kind: SharedString,
    /// Whatever the node's renderer needs. The built-in kinds read `label`.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    /// A fixed size. Without one the node is as large as its content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// The side the built-in kinds put their source handle on.
    #[serde(default = "bottom")]
    pub source_position: Side,
    /// The side the built-in kinds put their target handle on.
    #[serde(default = "top")]
    pub target_position: Side,
    #[serde(default, skip_serializing_if = "is_false")]
    pub selected: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// These four fall back to the flow's own switches when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draggable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletable: Option<bool>,
    /// The node this one lives in: it moves with its parent and is drawn above it.
    #[serde(
        rename = "parentId",
        alias = "parent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parent: Option<NodeId>,
    /// Where the node may be: inside its parent, or inside a box of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent: Option<Extent>,
    /// Higher is drawn later. A child is always drawn above its parent.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub z_index: i32,
    /// In a tree layout (a mind map), whether what's under the node is folded away.
    #[serde(default, skip_serializing_if = "is_false")]
    pub collapsed: bool,
    /// A whiteboard node's outline (the `shape` kind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<NodeShape>,
    /// A color from the flow's palette by name (`red`, `blue`, …); the node's own colors when
    /// unset or `default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<SharedString>,
    /// A Lucide icon by name (`database`), shown as a badge. One gpui-core doesn't bundle is
    /// left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<SharedString>,
    /// One emoji, shown as a badge beside the icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<SharedString>,
    /// How the node's text lines up; the kind's own way when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<LabelAlign>,
    /// An `image` node's picture: what the host's loader is asked for. The label is its
    /// caption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    /// A `link` node's card: the address, and what the host found there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<LinkCard>,
    /// What the node links to, as the host names it (a document's id, say). The flow shows a
    /// link button and tells the host when it's pressed; it never opens anything itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<SharedString>,
    /// Fields of React Flow's (`style`, `className`, `measured`, …) and any of the host's that
    /// this crate doesn't read. They're kept so a saved flow loads and saves without loss.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Where a node may be dragged to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
    /// Inside its parent's box. Saved as `"parent"`, as React Flow does.
    Parent,
    /// Inside this box of the flow. Saved as `[[x1, y1], [x2, y2]]`.
    Area(Rect),
}

impl Serialize for Extent {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Extent::Parent => s.serialize_str("parent"),
            Extent::Area(r) => [[r.x, r.y], [r.right(), r.bottom()]].serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Extent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Name(String),
            Corners([[f32; 2]; 2]),
        }
        match Wire::deserialize(d)? {
            Wire::Name(n) if n == "parent" => Ok(Extent::Parent),
            Wire::Name(n) => Err(serde::de::Error::custom(format!("unknown extent {n:?}"))),
            Wire::Corners([a, b]) => Ok(Extent::Area(Rect::from_corners(
                xy(a[0], a[1]),
                xy(b[0], b[1]),
            ))),
        }
    }
}

impl Node {
    pub fn new(id: impl Into<NodeId>, position: XY) -> Self {
        Self {
            id: id.into(),
            position,
            kind: default_kind(),
            data: Value::Null,
            width: None,
            height: None,
            source_position: Side::Bottom,
            target_position: Side::Top,
            selected: false,
            hidden: false,
            draggable: None,
            selectable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            z_index: 0,
            collapsed: false,
            shape: None,
            color: None,
            icon: None,
            emoji: None,
            text_align: None,
            media: None,
            card: None,
            link: None,
            extra: Map::new(),
        }
    }

    pub fn kind(mut self, kind: impl Into<SharedString>) -> Self {
        self.kind = kind.into();
        self
    }

    /// Sets `data.label`, which the built-in kinds show.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        if !self.data.is_object() {
            self.data = Value::Object(Default::default());
        }
        self.data["label"] = Value::String(label.into());
        self
    }

    pub fn data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn source_position(mut self, position: Side) -> Self {
        self.source_position = position;
        self
    }

    pub fn target_position(mut self, position: Side) -> Self {
        self.target_position = position;
        self
    }

    pub fn parent(mut self, parent: impl Into<NodeId>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Keeps the node inside its parent's box.
    pub fn extent_parent(mut self, on: bool) -> Self {
        self.extent = on.then_some(Extent::Parent);
        self
    }

    /// Keeps the node inside this box of the flow.
    pub fn extent(mut self, area: Rect) -> Self {
        self.extent = Some(Extent::Area(area));
        self
    }

    pub fn draggable(mut self, on: bool) -> Self {
        self.draggable = Some(on);
        self
    }

    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = Some(on);
        self
    }

    pub fn connectable(mut self, on: bool) -> Self {
        self.connectable = Some(on);
        self
    }

    pub fn deletable(mut self, on: bool) -> Self {
        self.deletable = Some(on);
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }

    /// Folds away what's under the node in a tree layout.
    pub fn collapsed(mut self, on: bool) -> Self {
        self.collapsed = on;
        self
    }

    /// A whiteboard node of this outline (of the `shape` kind).
    pub fn shape(mut self, shape: NodeShape) -> Self {
        self.kind = "shape".into();
        self.shape = Some(shape);
        self
    }

    /// Colors the node with the palette's color of this name.
    pub fn color(mut self, key: impl Into<SharedString>) -> Self {
        self.color = Some(key.into());
        self
    }

    /// Shows this Lucide icon on the node.
    pub fn icon(mut self, name: impl Into<SharedString>) -> Self {
        self.icon = Some(name.into());
        self
    }

    /// Shows this emoji on the node.
    pub fn emoji(mut self, emoji: impl Into<SharedString>) -> Self {
        self.emoji = Some(emoji.into());
        self
    }

    pub fn text_align(mut self, align: LabelAlign) -> Self {
        self.text_align = Some(align);
        self
    }

    /// An `image` node showing the picture the host's loader gives for `src`, `width` × `height`
    /// pixels (the node keeps those proportions), with `alt` saying what it shows.
    pub fn image(
        mut self,
        src: impl Into<SharedString>,
        width: f32,
        height: f32,
        alt: impl Into<SharedString>,
    ) -> Self {
        self.kind = "image".into();
        self.media = Some(Media {
            src: src.into(),
            width: Some(width),
            height: Some(height),
            alt: alt.into(),
        });
        self
    }

    /// A `link` node for this address, as a card the host fills in.
    pub fn link_card(mut self, url: impl Into<SharedString>) -> Self {
        self.kind = "link".into();
        self.card = Some(LinkCard {
            url: url.into(),
            ..LinkCard::default()
        });
        self
    }

    /// Links the node to something of the host's, by its id.
    pub fn link(mut self, link: impl Into<SharedString>) -> Self {
        self.link = Some(link.into());
        self
    }

    /// `data.label`, or nothing.
    pub fn label_text(&self) -> SharedString {
        self.data
            .get("label")
            .and_then(Value::as_str)
            .map(|s| SharedString::from(s.to_string()))
            .unwrap_or_default()
    }
}

/// A picture an `image` node shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    /// What the host's image loader is asked for: a file name, a document id, an address.
    /// The flow never fetches it itself.
    pub src: SharedString,
    /// The picture's own size in pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// What it shows, for someone who can't see it, and shown in its place until it loads.
    #[serde(default, skip_serializing_if = "is_blank")]
    pub alt: SharedString,
}

/// What a `link` node shows of an address: the address, and what the host found there (the
/// flow asks it with `FlowEvent::LinkCardNeeded`, and never fetches anything itself).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCard {
    pub url: SharedString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<SharedString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<SharedString>,
    /// The site's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<SharedString>,
    /// A preview picture, for the host's image loader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<SharedString>,
}

/// How a node's text lines up across it. (Not `TextAlign`, which GPUI exports too.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// How an edge's line runs between its two handles.
///
/// Saved under React Flow's names: `default`, `straight`, `step`, `smoothstep` and
/// `simplebezier`. Any other name is kept as [`EdgeKind::Custom`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// A curve that leaves and arrives square to the handles.
    #[default]
    Bezier,
    /// One straight line.
    Straight,
    /// Right angles.
    Step,
    /// Right angles with rounded corners.
    SmoothStep,
    /// A bezier whose bend doesn't grow when the target is behind the source.
    SimpleBezier,
    /// A kind of the host's, by name. It's drawn as a [`EdgeKind::Bezier`].
    Custom(SharedString),
}

impl EdgeKind {
    /// React Flow's name for it.
    pub fn name(&self) -> &str {
        match self {
            EdgeKind::Bezier => "default",
            EdgeKind::Straight => "straight",
            EdgeKind::Step => "step",
            EdgeKind::SmoothStep => "smoothstep",
            EdgeKind::SimpleBezier => "simplebezier",
            EdgeKind::Custom(name) => name,
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "default" | "bezier" => EdgeKind::Bezier,
            "straight" => EdgeKind::Straight,
            "step" => EdgeKind::Step,
            "smoothstep" => EdgeKind::SmoothStep,
            "simplebezier" => EdgeKind::SimpleBezier,
            other => EdgeKind::Custom(SharedString::from(other.to_string())),
        }
    }
}

impl Serialize for EdgeKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for EdgeKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(EdgeKind::from_name(&String::deserialize(d)?))
    }
}

/// How an edge's line is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeDash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl EdgeDash {
    /// As React Flow (CSS) gives it: a dash array.
    pub fn dasharray(self) -> Option<&'static str> {
        match self {
            EdgeDash::Solid => None,
            EdgeDash::Dashed => Some("6 4"),
            EdgeDash::Dotted => Some("1 4"),
        }
    }

    /// From a CSS dash array: dotted when the dashes are much shorter than the gaps, dashed
    /// otherwise, and solid for none.
    pub fn from_dasharray(value: &Value) -> Self {
        let text = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return EdgeDash::Solid,
        };
        let parts: Vec<f32> = text
            .split([',', ' '])
            .filter_map(|p| p.trim().trim_end_matches("px").parse().ok())
            .filter(|n: &f32| n.is_finite() && *n >= 0.)
            .collect();
        match parts.as_slice() {
            [] => EdgeDash::Solid,
            [d] if *d == 0. => EdgeDash::Solid,
            [d] => {
                if *d <= 2. {
                    EdgeDash::Dotted
                } else {
                    EdgeDash::Dashed
                }
            }
            [d, gap, ..] => {
                if *d == 0. && *gap == 0. {
                    EdgeDash::Solid
                } else if *d * 2. <= *gap {
                    EdgeDash::Dotted
                } else {
                    EdgeDash::Dashed
                }
            }
        }
    }
}

/// Which ends of an edge have arrowheads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Arrows {
    None,
    /// At the target.
    #[default]
    Forward,
    /// At the source.
    Reverse,
    Both,
}

/// The arrowhead's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkerKind {
    /// Two open strokes.
    #[default]
    Arrow,
    /// A filled triangle.
    ArrowClosed,
}

fn marker_size() -> f32 {
    10.
}

/// React Flow's markers are 12.5 units on a side by default; this crate's arrow is 10 long.
const MARKER_SCALE: f32 = 0.8;

/// An arrowhead at an end of an edge, pointing along it.
///
/// Saved as React Flow's `{ "type", "color", "width", "height" }`. A marker given by name (an
/// SVG id in React Flow) loads as no marker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub kind: MarkerKind,
    /// The arrow's length in flow units.
    pub size: f32,
    /// `0xRRGGBB`; the edge's own color when unset.
    pub color: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct MarkerWire {
    #[serde(rename = "type", default)]
    kind: MarkerKind,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "color_serde")]
    color: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    height: Option<f32>,
}

impl Serialize for Marker {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let side = (self.size != marker_size()).then_some(self.size / MARKER_SCALE);
        MarkerWire {
            kind: self.kind,
            color: self.color,
            width: side,
            height: side,
        }
        .serialize(s)
    }
}

/// A marker as React Flow saves it: an object, or a name this crate can't draw.
fn marker_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Marker>, D::Error> {
    let value = Value::deserialize(d)?;
    if !value.is_object() {
        return Ok(None);
    }
    let wire: MarkerWire = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
    Ok(Some(Marker {
        kind: wire.kind,
        size: wire
            .width
            .or(wire.height)
            .filter(|s| s.is_finite() && *s > 0.)
            .map_or(marker_size(), |s| s * MARKER_SCALE),
        color: wire.color,
    }))
}

impl<'de> Deserialize<'de> for Marker {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        marker_de(d)?.ok_or_else(|| serde::de::Error::custom("a marker is an object"))
    }
}

impl Marker {
    pub fn arrow() -> Self {
        Self {
            kind: MarkerKind::Arrow,
            size: marker_size(),
            color: None,
        }
    }

    pub fn arrow_closed() -> Self {
        Self {
            kind: MarkerKind::ArrowClosed,
            size: marker_size(),
            color: None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, rgb: u32) -> Self {
        self.color = Some(rgb);
        self
    }
}

/// A CSS color as `0xRRGGBB`: `#rgb`, `#rrggbb`, `#rrggbbaa` (the alpha is dropped),
/// `rgb(r, g, b)`, or a number. None for anything else (names, variables).
pub fn parse_color(value: &Value) -> Option<u32> {
    if let Some(n) = value.as_u64() {
        return u32::try_from(n).ok().filter(|n| *n <= 0xff_ffff);
    }
    let s = value.as_str()?.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let digits = |h: &str| u32::from_str_radix(h, 16).ok();
        return match hex.len() {
            3 | 4 => {
                let v = digits(&hex[..3])?;
                let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
                Some((r * 17) << 16 | (g * 17) << 8 | (b * 17))
            }
            6 | 8 => digits(&hex[..6]),
            _ => None,
        };
    }
    let inner = s
        .strip_prefix("rgba(")
        .or_else(|| s.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let mut parts = inner.split([',', ' ']).filter(|p| !p.is_empty());
    let mut channel = || {
        parts
            .next()?
            .trim()
            .parse::<f32>()
            .ok()
            .map(|c| c.clamp(0., 255.) as u32)
    };
    let (r, g, b) = (channel()?, channel()?, channel()?);
    Some(r << 16 | g << 8 | b)
}

/// A color saved as CSS, `#rrggbb`.
pub fn color_css(rgb: u32) -> String {
    format!("#{rgb:06x}")
}

mod color_serde {
    use super::*;

    pub fn serialize<S: serde::Serializer>(c: &Option<u32>, s: S) -> Result<S::Ok, S::Error> {
        match c {
            Some(c) => s.serialize_str(&color_css(*c)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
        Ok(parse_color(&Value::deserialize(d)?))
    }
}

/// An edge: a line from one node's source handle to another's target handle.
///
/// Saved as React Flow saves it. The line's color and width are its `style`'s `stroke` and
/// `strokeWidth`; the rest of `style`, and any other field this crate doesn't read, is kept in
/// `extra`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "EdgeWire", into = "EdgeWire")]
pub struct Edge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    /// Which of the source node's handles, when it has several with ids.
    pub source_handle: Option<SharedString>,
    pub target_handle: Option<SharedString>,
    pub kind: EdgeKind,
    pub label: Option<SharedString>,
    /// Dashes that run from the source to the target.
    pub animated: bool,
    pub selected: bool,
    pub hidden: bool,
    pub marker_start: Option<Marker>,
    pub marker_end: Option<Marker>,
    /// `0xRRGGBB`; the theme's edge color when unset.
    pub color: Option<u32>,
    /// The line's width in flow units; 1 when unset.
    pub stroke_width: Option<f32>,
    /// Solid, dashed or dotted. Saved as React Flow saves it, in `style.strokeDasharray`.
    pub dash: EdgeDash,
    /// A color from the flow's palette by name (`blue`), saved as `color` as subcanvas saves
    /// it: the line, its arrowheads and its label take it. An exact `color` wins over it.
    pub pencil: Option<SharedString>,
    pub selectable: Option<bool>,
    pub deletable: Option<bool>,
    /// Higher is drawn later. An edge to a child node is drawn at that node's level.
    pub z_index: i32,
    /// How wide (in flow units) the band around the line is that takes clicks; 20 when unset.
    pub interaction_width: Option<f32>,
    pub data: Value,
    /// Fields this crate doesn't read, kept for saving. The rest of `style` is under `style`.
    pub extra: Map<String, Value>,
}

/// A label as React Flow saves it, where it can be any React node: text and numbers are kept.
fn label_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<SharedString>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => Some(s.into()),
        Value::Number(n) => Some(n.to_string().into()),
        Value::Bool(b) => Some(b.to_string().into()),
        _ => None,
    })
}

fn number_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f32>, D::Error> {
    Ok(number(&Value::deserialize(d)?))
}

/// A number, or a string holding one ("2", "2px").
fn number(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n) => n.as_f64().map(|n| n as f32),
        Value::String(s) => s.trim().trim_end_matches("px").parse().ok(),
        _ => None,
    }
    .filter(|n: &f32| n.is_finite())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeWire {
    id: EdgeId,
    source: NodeId,
    target: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_handle: Option<SharedString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_handle: Option<SharedString>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    kind: Option<EdgeKind>,
    #[serde(
        default,
        deserialize_with = "label_de",
        skip_serializing_if = "Option::is_none"
    )]
    label: Option<SharedString>,
    #[serde(default, skip_serializing_if = "is_false")]
    animated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    selected: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    hidden: bool,
    #[serde(
        default,
        deserialize_with = "marker_de",
        skip_serializing_if = "Option::is_none"
    )]
    marker_start: Option<Marker>,
    #[serde(
        default,
        deserialize_with = "marker_de",
        skip_serializing_if = "Option::is_none"
    )]
    marker_end: Option<Marker>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    style: Map<String, Value>,
    /// A palette color by name, as subcanvas saves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<SharedString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deletable: Option<bool>,
    #[serde(default, skip_serializing_if = "is_zero")]
    z_index: i32,
    #[serde(
        default,
        deserialize_with = "number_de",
        skip_serializing_if = "Option::is_none"
    )]
    interaction_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    data: Value,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl From<EdgeWire> for Edge {
    fn from(w: EdgeWire) -> Self {
        let mut style = w.style;
        let color = style.get("stroke").and_then(parse_color);
        if color.is_some() {
            style.remove("stroke");
        }
        let stroke_width = style.get("strokeWidth").and_then(number);
        if stroke_width.is_some() {
            style.remove("strokeWidth");
        }
        // The dash array itself stays in `style`, so it's saved again as it came while it still
        // means the same dash.
        let dash = style
            .get("strokeDasharray")
            .map(EdgeDash::from_dasharray)
            .unwrap_or_default();
        let mut extra = w.extra;
        if !style.is_empty() {
            extra.insert("style".into(), Value::Object(style));
        }
        Edge {
            id: w.id,
            source: w.source,
            target: w.target,
            // React Flow treats an empty handle id as none.
            source_handle: w.source_handle.filter(|h| !h.is_empty()),
            target_handle: w.target_handle.filter(|h| !h.is_empty()),
            kind: w.kind.unwrap_or_default(),
            label: w.label,
            animated: w.animated,
            selected: w.selected,
            hidden: w.hidden,
            marker_start: w.marker_start,
            marker_end: w.marker_end,
            color,
            stroke_width,
            dash,
            pencil: w.color,
            selectable: w.selectable,
            deletable: w.deletable,
            z_index: w.z_index,
            interaction_width: w.interaction_width,
            data: w.data,
            extra,
        }
    }
}

impl From<Edge> for EdgeWire {
    fn from(e: Edge) -> Self {
        let mut extra = e.extra;
        let mut style = match extra.remove("style") {
            Some(Value::Object(style)) => style,
            _ => Map::new(),
        };
        if let Some(c) = e.color {
            style.insert("stroke".into(), Value::String(color_css(c)));
        }
        if let Some(w) = e.stroke_width.and_then(serde_json::Number::from_f64_lossy) {
            style.insert("strokeWidth".into(), Value::Number(w));
        }
        let kept = style
            .get("strokeDasharray")
            .is_some_and(|d| EdgeDash::from_dasharray(d) == e.dash);
        if !kept {
            match e.dash.dasharray() {
                Some(d) => style.insert("strokeDasharray".into(), Value::String(d.into())),
                None => style.remove("strokeDasharray"),
            };
        }
        EdgeWire {
            id: e.id,
            source: e.source,
            target: e.target,
            source_handle: e.source_handle,
            target_handle: e.target_handle,
            kind: Some(e.kind),
            label: e.label,
            animated: e.animated,
            selected: e.selected,
            hidden: e.hidden,
            marker_start: e.marker_start,
            marker_end: e.marker_end,
            style,
            color: e.pencil,
            selectable: e.selectable,
            deletable: e.deletable,
            z_index: e.z_index,
            interaction_width: e.interaction_width,
            data: e.data,
            extra,
        }
    }
}

trait LossyNumber {
    fn from_f64_lossy(v: f32) -> Option<serde_json::Number>;
}

impl LossyNumber for serde_json::Number {
    /// The f32 as the shortest decimal that reads back as it, not its f64 widening (1.5, not
    /// 1.5000000xx).
    fn from_f64_lossy(v: f32) -> Option<serde_json::Number> {
        serde_json::Number::from_f64(v.to_string().parse::<f64>().ok()?)
    }
}

impl Edge {
    pub fn new(
        id: impl Into<EdgeId>,
        source: impl Into<NodeId>,
        target: impl Into<NodeId>,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            source_handle: None,
            target_handle: None,
            kind: EdgeKind::default(),
            label: None,
            animated: false,
            selected: false,
            hidden: false,
            marker_start: None,
            marker_end: None,
            color: None,
            stroke_width: None,
            dash: EdgeDash::Solid,
            pencil: None,
            selectable: None,
            deletable: None,
            z_index: 0,
            interaction_width: None,
            data: Value::Null,
            extra: Map::new(),
        }
    }

    pub fn kind(mut self, kind: EdgeKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn animated(mut self, on: bool) -> Self {
        self.animated = on;
        self
    }

    pub fn source_handle(mut self, id: impl Into<SharedString>) -> Self {
        self.source_handle = Some(id.into());
        self
    }

    pub fn target_handle(mut self, id: impl Into<SharedString>) -> Self {
        self.target_handle = Some(id.into());
        self
    }

    pub fn marker_start(mut self, marker: Marker) -> Self {
        self.marker_start = Some(marker);
        self
    }

    pub fn marker_end(mut self, marker: Marker) -> Self {
        self.marker_end = Some(marker);
        self
    }

    pub fn color(mut self, rgb: u32) -> Self {
        self.color = Some(rgb);
        self
    }

    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = Some(width);
        self
    }

    pub fn dash(mut self, dash: EdgeDash) -> Self {
        self.dash = dash;
        self
    }

    /// Colors the edge with the palette's color of this name.
    pub fn pencil(mut self, key: impl Into<SharedString>) -> Self {
        self.pencil = Some(key.into());
        self
    }

    /// Arrowheads at neither end, the target's, the source's, or both: closed ones, as
    /// subcanvas draws them.
    pub fn arrows(mut self, arrows: Arrows) -> Self {
        let (start, end) = match arrows {
            Arrows::None => (false, false),
            Arrows::Forward => (false, true),
            Arrows::Reverse => (true, false),
            Arrows::Both => (true, true),
        };
        self.marker_start = start.then(Marker::arrow_closed);
        self.marker_end = end.then(Marker::arrow_closed);
        self
    }

    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = Some(on);
        self
    }

    pub fn deletable(mut self, on: bool) -> Self {
        self.deletable = Some(on);
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }

    pub fn data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }
}

/// A link someone is making or has made by dragging between two handles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub source: NodeId,
    pub source_handle: Option<SharedString>,
    pub target: NodeId,
    pub target_handle: Option<SharedString>,
}

/// Which handles a drag may end on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionMode {
    /// A source handle only connects to a target handle, and the other way round.
    #[default]
    Strict,
    /// Any handle connects to any other.
    Loose,
}

/// What a selection rectangle takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    /// Nodes wholly inside it.
    #[default]
    Full,
    /// Nodes it touches.
    Partial,
}

/// A whole flow as data, for saving and loading.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowObject {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub viewport: Viewport,
}
