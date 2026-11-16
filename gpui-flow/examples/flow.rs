//! The flow canvas in a window, with one of everything it can do.
//!
//! ```sh
//! cargo run --release --example flow
//! cargo run --release --example flow -- --bench [--nodes 500]
//! cargo run --release --features shots --example flow -- --shot out.png [--scene connect]
//! ```
//!
//! Drag the pane to pan it, scroll or pinch to zoom, drag nodes, drag from one handle to
//! another to connect, shift-drag for a selection rectangle, ⌘-click to add to the selection,
//! delete to remove, ⌘A, ⌘+, ⌘−, ⌘0, ⇧1 to fit. `--shot` takes a picture of the window without
//! showing it: `overview` (everything), `connect` (a connection being dragged), `select` (a
//! selection rectangle), `zoom` (zoomed in on the custom nodes) or `lines` (the lined pattern).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::Theme;
use gpui_flow::*;
use serde_json::json;

struct Args {
    shot: Option<PathBuf>,
    scene: String,
    /// Pans and zooms by itself, prints how long frames took, and quits.
    bench: bool,
    /// A grid of this many plain nodes, each joined to the next, in place of the sample.
    nodes: usize,
    /// A mind map in place of the sample: tab, enter and the arrows grow and walk it.
    mindmap: bool,
    /// A whiteboard of shapes in place of the sample: every outline, palette color and badge.
    whiteboard: bool,
}

fn args() -> Args {
    let mut out = Args {
        shot: None,
        scene: "overview".into(),
        bench: false,
        nodes: 0,
        mindmap: false,
        whiteboard: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shot" => out.shot = it.next().map(PathBuf::from),
            "--scene" => out.scene = it.next().unwrap_or_default(),
            "--bench" => out.bench = true,
            "--mindmap" => out.mindmap = true,
            "--whiteboard" => out.whiteboard = true,
            "--nodes" => out.nodes = it.next().and_then(|n| n.parse().ok()).unwrap_or(0),
            _ => {}
        }
    }
    out
}

/// A host app's charcoal theme, with its hover wash (the foreground at 5%).
fn apply_theme(cx: &mut App) {
    let mut t = Theme::dark();
    t.accent = rgba(0xededef0d).into();
    Theme::set(t, cx);
}

const GREEN: u32 = 0x4cc38a;
const RED: u32 = 0xe5665b;
const AMBER: u32 = 0xe2a336;

fn sample_nodes() -> Vec<Node> {
    vec![
        Node::new("in", xy(260., 0.)).kind("input").label("Email arrives"),
        Node::new("triage", xy(235., 110.))
            .kind("agent")
            .data(json!({ "name": "Main", "role": "Sorts what came in", "mark": "🧭" })),
        Node::new("finances", xy(560., 40.))
            .kind("agent")
            .data(json!({ "name": "Finances", "role": "Bills and receipts", "mark": "💳" })),
        Node::new("school", xy(560., 180.))
            .kind("agent")
            .data(json!({ "name": "School", "role": "Deadlines and forms", "mark": "🎓" })),
        Node::new("ask", xy(0., 130.)).label("Ask Trevin").source_position(Side::Right),
        Node::new("note", xy(-10., 300.))
            .kind("note")
            .size(190., 84.)
            .data(json!({ "text": "Select me and drag a corner. Everything on the canvas is an ordinary GPUI element." })),
        Node::new("steps", xy(300., 300.)).kind("group").size(330., 150.).label("Approval"),
        Node::new("draft", xy(15., 35.)).parent("steps").extent_parent(true).label("Draft a reply").size(130., 38.),
        Node::new("approve", xy(180., 85.))
            .parent("steps")
            .extent_parent(true)
            .label("Wait for a yes")
            .size(130., 38.)
            .target_position(Side::Left),
        Node::new("out", xy(690., 300.))
            .kind("output")
            .label("Send")
            .target_position(Side::Left),
    ]
}

fn sample_edges() -> Vec<Edge> {
    vec![
        Edge::new("in-triage", "in", "triage")
            .target_handle("in")
            .animated(true),
        Edge::new("triage-finances", "triage", "finances")
            .source_handle("ok")
            .target_handle("in")
            .label("a bill")
            .marker_end(Marker::arrow_closed()),
        Edge::new("triage-school", "triage", "school")
            .source_handle("fail")
            .target_handle("in")
            .kind(EdgeKind::SmoothStep)
            .label("a form")
            .marker_end(Marker::arrow_closed()),
        Edge::new("ask-triage", "ask", "triage")
            .target_handle("in")
            .kind(EdgeKind::Straight)
            .color(AMBER)
            .marker_end(Marker::arrow()),
        Edge::new("triage-draft", "triage", "draft")
            .source_handle("down")
            .kind(EdgeKind::SimpleBezier),
        Edge::new("draft-approve", "draft", "approve").kind(EdgeKind::Step),
        Edge::new("approve-out", "approve", "out")
            .kind(EdgeKind::SmoothStep)
            .label("approved")
            .color(GREEN)
            .stroke_width(1.5)
            .marker_end(Marker::arrow_closed()),
    ]
}

/// A custom kind: an agent's card, with one way in, two ways out on the right (each its own
/// handle with an id) and one out of the bottom, and a button that pauses the agent.
fn agent_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let text =
        |key: &str| SharedString::from(node.data[key].as_str().unwrap_or_default().to_string());
    let paused = node.data["paused"].as_bool().unwrap_or(false);
    // `nodrag`: pressing the button neither selects nor drags the card. Its handler changes
    // the node through the flow, which it can reach then (not while the node is drawn).
    let toggle = {
        let (flow, id) = (ctx.flow().clone(), node.id.clone());
        div()
            .id(SharedString::from(format!("pause-{}", node.id)))
            .nodrag()
            .ml_auto()
            .px(units(7.))
            .py(units(2.))
            .rounded(units(4.))
            .border_1()
            .border_color(ctx.colors.node_border)
            .text_size(ctx.font_size(9.))
            .text_color(ctx.colors.muted_text)
            .cursor(CursorStyle::PointingHand)
            .hover(|s| s.bg(ctx.colors.group))
            .child(if paused { "Resume" } else { "Pause" })
            .on_click(move |_, _, cx| {
                flow.update(cx, |flow, cx| {
                    flow.update_node(&id, |n| n.data["paused"] = json!(!paused), cx);
                })
                .ok();
            })
    };
    div()
        .relative()
        .w(units(190.))
        .rounded(units(8.))
        .border_1()
        .border_color(if ctx.selected {
            ctx.colors.selected
        } else {
            ctx.colors.node_border
        })
        .bg(ctx.colors.node)
        .child(
            div()
                .flex()
                .items_center()
                .gap(units(9.))
                .p(units(10.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(units(30.))
                        .rounded(units(7.))
                        .bg(ctx.colors.group)
                        .text_size(ctx.font_size(15.))
                        .child(text("mark")),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().font_weight(FontWeight::MEDIUM).child(text("name")))
                        .child(
                            div()
                                .text_size(ctx.font_size(10.))
                                .text_color(ctx.colors.muted_text)
                                .child(if paused {
                                    "Paused".into()
                                } else {
                                    text("role")
                                }),
                        ),
                )
                .child(toggle),
        )
        .child(ctx.handle(HandleKind::Target, Side::Left).id("in"))
        .child(
            ctx.handle(HandleKind::Source, Side::Right)
                .id("ok")
                .at(0.3)
                .color(rgb(GREEN)),
        )
        .child(
            ctx.handle(HandleKind::Source, Side::Right)
                .id("fail")
                .at(0.7)
                .color(rgb(RED)),
        )
        .child(ctx.handle(HandleKind::Source, Side::Bottom).id("down"))
        .into_any_element()
}

/// A custom kind that takes whatever size the node has, with the resizer to change it.
fn note_node(node: &Node, ctx: &NodeContext) -> AnyElement {
    let amber: Hsla = rgb(AMBER).into();
    div()
        .relative()
        .size_full()
        .p(units(10.))
        .rounded(units(4.))
        .border_1()
        .border_color(amber.opacity(0.5))
        .bg(amber.opacity(0.12))
        .text_size(ctx.font_size(11.))
        .child(SharedString::from(
            node.data["text"].as_str().unwrap_or_default().to_string(),
        ))
        .child(ctx.resizer(120., 50.))
        .into_any_element()
}

/// A mind map of planning a trip: a root with ideas both ways, one of them folded away.
/// A whiteboard sketch of a sign-up service: one of each shape, in the palette's colors, some
/// with an icon or an emoji.
fn whiteboard() -> (Vec<Node>, Vec<Edge>) {
    let at = |x: f32, y: f32| xy(x, y);
    let nodes = vec![
        Node::new("user", at(0., 120.))
            .shape(NodeShape::Ellipse)
            .label("Someone signs up")
            .emoji("👋"),
        Node::new("form", at(240., 120.))
            .shape(NodeShape::Parallelogram)
            .label("Email and password")
            .color("blue"),
        Node::new("valid", at(480., 96.))
            .shape(NodeShape::Diamond)
            .label("Valid?")
            .color("yellow"),
        Node::new("storage", at(700., 40.))
            .kind("frame")
            .label("Storage")
            .icon("hard-drive")
            .size(260., 420.),
        Node::new("db", at(40., 60.))
            .parent("storage")
            .shape(NodeShape::Cylinder)
            .label("Users")
            .icon("database")
            .color("teal"),
        Node::new("mail", at(20., 260.))
            .parent("storage")
            .shape(NodeShape::Document)
            .label("Welcome email")
            .icon("mail")
            .color("purple"),
        Node::new("api", at(480., 300.))
            .shape(NodeShape::Hexagon)
            .label("Auth service")
            .icon("shield-check")
            .color("green"),
        Node::new("cdn", at(220., 320.))
            .shape(NodeShape::Cloud)
            .label("Sent from the edge")
            .color("orange"),
        Node::new("error", at(480., -60.))
            .shape(NodeShape::Rounded)
            .label("Show what's wrong")
            .color("red")
            .text_align(LabelAlign::Left),
        Node::new("note", at(0., 320.))
            .shape(NodeShape::Rectangle)
            .label("Rate-limit sign-ups from one address")
            .color("pink")
            .text_align(LabelAlign::Left),
    ];
    let edge = |id: &str, s: &str, sh: &str, t: &str, th: &str| {
        Edge::new(id, s, t)
            .source_handle(sh)
            .target_handle(th)
            .kind(EdgeKind::SmoothStep)
            .marker_end(Marker::arrow_closed())
    };
    let edges = vec![
        edge("e1", "user", "right", "form", "left"),
        edge("e2", "form", "right", "valid", "left"),
        edge("e3", "valid", "right", "db", "left").label("yes"),
        edge("e4", "valid", "top", "error", "bottom")
            .label("no")
            .pencil("red")
            .dash(EdgeDash::Dashed),
        edge("e5", "db", "bottom", "mail", "top"),
        edge("e6", "valid", "bottom", "api", "top"),
        // Past the note and the cloud, which the route goes round.
        edge("e7", "user", "bottom", "api", "left")
            .label("checks the limit")
            .pencil("pink")
            .dash(EdgeDash::Dotted),
    ];
    (nodes, edges)
}

fn mind_map() -> (Vec<Node>, Vec<Edge>) {
    let topics = [
        ("trip", "Kyoto in April", None),
        ("stay", "Where to stay", Some("trip")),
        ("ryokan", "A ryokan in Higashiyama", Some("stay")),
        ("hotel", "A hotel by the station", Some("stay")),
        ("see", "What to see", Some("trip")),
        ("temples", "Temples at dawn", Some("see")),
        ("bamboo", "Arashiyama's bamboo", Some("see")),
        ("blossom", "Cherry blossom", Some("see")),
        ("eat", "Where to eat", Some("trip")),
        ("market", "Nishiki market", Some("eat")),
        ("kaiseki", "One kaiseki dinner", Some("eat")),
        ("get", "Getting there", Some("trip")),
        ("rail", "A rail pass", Some("get")),
        ("flights", "Flights", Some("get")),
    ];
    let nodes = topics
        .iter()
        .map(|(id, label, _)| {
            let n = Node::new(*id, xy(0., 0.)).kind("topic").label(*label);
            // A topic can link to a document of the host's; the flow only reports the press.
            let n = if *id == "ryokan" {
                n.link("doc:ryokan-booking")
            } else {
                n
            };
            if *id == "get" { n.collapsed(true) } else { n }
        })
        .collect();
    let edges = topics
        .iter()
        .filter_map(|(id, _, parent)| {
            let parent = (*parent)?;
            Some(Edge::new(format!("{parent}-{id}"), parent, *id))
        })
        .collect();
    (nodes, edges)
}

/// A grid of plain nodes, each joined to the next: something large to measure with.
fn grid(count: usize) -> (Vec<Node>, Vec<Edge>) {
    let columns = (count as f32).sqrt().ceil() as usize;
    let nodes = (0..count)
        .map(|i| {
            let at = xy((i % columns) as f32 * 200., (i / columns) as f32 * 110.);
            Node::new(format!("n{i}"), at).label(format!("Node {i}"))
        })
        .collect();
    let edges = (1..count)
        .map(|i| Edge::new(format!("e{i}"), format!("n{}", i - 1), format!("n{i}")))
        .collect();
    (nodes, edges)
}

/// `--bench`: the flow moved a little on every frame, and how long the frames took.
struct Bench {
    frame: usize,
    last: Option<Instant>,
    base: Viewport,
    /// The time between one frame's start and the next's.
    intervals: Vec<(usize, f32)>,
    /// The time from a frame's start to the end of its painting: the app's own work.
    work: Rc<RefCell<Vec<(usize, f32)>>>,
    started: Rc<Cell<(usize, Option<Instant>)>>,
}

const WARM_UP: usize = 40;
const PHASE: usize = 240;

fn report(name: &str, what: &str, values: impl Iterator<Item = f32>) {
    let mut v: Vec<f32> = values.collect();
    if v.is_empty() {
        return;
    }
    v.sort_by(f32::total_cmp);
    let at = |q: f32| v[((v.len() - 1) as f32 * q) as usize];
    println!(
        "{name:<5} {what:<9} median {:5.2} ms  p95 {:5.2} ms  max {:5.2} ms  ({:.0} fps at the median)",
        at(0.5),
        at(0.95),
        at(1.),
        1000. / at(0.5)
    );
}

struct Root {
    bench: Option<Bench>,
    flow: Entity<Flow>,
    /// The last thing the flow said happened, shown at the bottom.
    status: SharedString,
    saved: Option<FlowObject>,
    variant: usize,
    added: usize,
    _subscription: Subscription,
}

impl Root {
    fn new(args: &Args, cx: &mut Context<Self>) -> Self {
        let root = cx.entity().downgrade();
        let mindmap = args.mindmap;
        let (nodes, edges) = if args.nodes > 0 {
            grid(args.nodes)
        } else if args.whiteboard {
            whiteboard()
        } else if mindmap {
            mind_map()
        } else {
            (sample_nodes(), sample_edges())
        };
        let flow = cx.new(|cx| {
            let (top, bottom) = (root.clone(), root.clone());
            Flow::new(cx)
                .with_nodes(nodes)
                .with_edges(edges)
                .with_node_kind("agent", |node, ctx, _, _, _| agent_node(node, ctx))
                .with_node_kind("note", |node, ctx, _, _, _| note_node(node, ctx))
                .with_options(|o| {
                    o.min_zoom = 0.2;
                    o.max_zoom = 4.;
                    o.transition = std::time::Duration::from_millis(180);
                    o.edge_defaults.marker_end = Some(Marker::arrow_closed());
                    o.avoid_nodes = true;
                    if mindmap {
                        o.layout =
                            Layout::Tree(TreeLayout::default().direction(TreeDirection::Both));
                        o.edge_defaults.marker_end = None;
                    }
                })
                // Nothing may connect into a node from itself.
                .with_connection_check(|c, _| c.source != c.target)
                .with_background(FlowBackground::dots())
                .with_controls()
                .with_minimap()
                .with_panel(PanelPosition::TopRight, move |flow, _, cx| {
                    toolbar(&top, flow, cx)
                })
                .with_panel(PanelPosition::BottomCenter, move |_, _, cx| {
                    status_line(&bottom, cx)
                })
                .fit_view_on_start()
        });
        let subscription = cx.subscribe(&flow, |this, flow, event: &FlowEvent, cx| {
            let said = match event {
                FlowEvent::Connect(edge) => format!("Connected {} → {}", edge.source, edge.target),
                FlowEvent::NodeClick(id) => format!("Clicked {id}"),
                FlowEvent::NodeDoubleClick(id) => format!("Double-clicked {id}"),
                FlowEvent::NodeDragStop(ids) | FlowEvent::NodeMove(ids) => {
                    format!("Moved {}", ids.join(", "))
                }
                FlowEvent::NodeResizeEnd(id) => format!("Resized {id}"),
                FlowEvent::EdgeClick(id) => format!("Clicked edge {id}"),
                FlowEvent::PaneClick(at) => format!("Clicked the pane at {:.0}, {:.0}", at.x, at.y),
                FlowEvent::SelectionChange { nodes, edges } => {
                    format!("{} nodes and {} edges selected", nodes.len(), edges.len())
                }
                FlowEvent::Delete { nodes, edges } => {
                    format!("Deleted {} nodes and {} edges", nodes.len(), edges.len())
                }
                FlowEvent::OpenLink { link, .. } => format!("Open {link}"),
                FlowEvent::ContextMenu { target, .. } => match target {
                    ContextTarget::Node(id) => format!("Menu for {id}"),
                    ContextTarget::Edge(id) => format!("Menu for edge {id}"),
                    ContextTarget::Pane => "Menu for the pane".into(),
                },
                FlowEvent::ConnectEnd {
                    connected: false, ..
                } => "No connection made".into(),
                _ => return,
            };
            this.status = said.into();
            // The status line is a panel of the flow's, so it's the flow that draws again.
            flow.update(cx, |_, cx| cx.notify());
        });
        Self {
            bench: args.bench.then(|| Bench {
                frame: 0,
                last: None,
                base: Viewport::default(),
                intervals: Vec::new(),
                work: Rc::default(),
                started: Rc::default(),
            }),
            flow,
            status: if args.mindmap {
                "Tab adds a child, enter a sibling; the arrows move; drop a topic on another to move it"
            } else {
                "Drag from a handle to connect; shift-drag to select; delete removes"
            }
            .into(),
            saved: None,
            variant: 0,
            added: 0,
            _subscription: subscription,
        }
    }

    fn add_node(&mut self, cx: &mut Context<Self>) {
        self.added += 1;
        let n = self.added;
        self.flow.update(cx, |flow, cx| {
            // In the middle of what the pane shows, each a little further along.
            let v = flow.viewport();
            let at = v.to_flow(xy(560. + 24. * n as f32, 330. + 24. * n as f32));
            flow.add_node(
                Node::new(format!("new-{n}"), at).label(format!("Node {n}")),
                cx,
            );
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let object = self.flow.read(cx).to_object();
        let bytes = serde_json::to_string(&object).map_or(0, |s| s.len());
        self.status = format!(
            "Saved {} nodes and {} edges ({bytes} bytes of JSON)",
            object.nodes.len(),
            object.edges.len()
        )
        .into();
        self.saved = Some(object);
        self.flow.update(cx, |_, cx| cx.notify());
    }

    fn restore(&mut self, cx: &mut Context<Self>) {
        if let Some(object) = self.saved.clone() {
            self.flow.update(cx, |flow, cx| flow.restore(object, cx));
        }
    }
}

fn button(label: &'static str, on: bool, act: impl Fn(&mut App) + 'static) -> Div {
    div()
        .px(px(9.))
        .py(px(4.))
        .rounded(px(5.))
        .cursor(CursorStyle::PointingHand)
        .text_size(px(12.))
        .hover(|s| s.bg(rgba(0xededef14)))
        .when(on, |d| d.bg(rgba(0xededef1f)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| act(cx))
        .child(label)
}

/// The top right panel: things the host can do to the flow.
fn toolbar(root: &WeakEntity<Root>, this: &Flow, cx: &mut App) -> AnyElement {
    let Some(entity) = root.upgrade() else {
        return div().into_any_element();
    };
    let c = Theme::global(cx).colors;
    let (flow, variant, has_saved) = {
        let r = entity.read(cx);
        (r.flow.clone(), r.variant, r.saved.is_some())
    };
    let options = this.options.clone();
    let on_root = |act: fn(&mut Root, &mut Context<Root>)| {
        let root = root.clone();
        move |cx: &mut App| {
            root.update(cx, act).ok();
        }
    };
    let on_flow = |act: fn(&mut Flow, &mut Context<Flow>)| {
        let flow = flow.clone();
        move |cx: &mut App| {
            flow.update(cx, |f, cx| {
                act(f, cx);
                cx.notify();
            })
        }
    };
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(c.border)
        .bg(c.popover)
        .text_color(c.foreground)
        .child(button("Add node", false, on_root(|r, cx| r.add_node(cx))))
        .child(button(
            ["Dots", "Lines", "Cross", "Plain"][variant],
            false,
            on_root(|r, cx| {
                r.variant = (r.variant + 1) % 4;
                let variant = r.variant;
                r.flow.update(cx, |f, cx| {
                    f.set_background(
                        [
                            Some(FlowBackground::dots()),
                            Some(FlowBackground::lines()),
                            Some(FlowBackground::cross()),
                            None,
                        ][variant],
                        cx,
                    )
                });
            }),
        ))
        .child(button(
            "Snap",
            options.snap_to_grid,
            on_flow(|f, _| f.options.snap_to_grid = !f.options.snap_to_grid),
        ))
        .child(button(
            "Scroll pans",
            options.pan_on_scroll,
            on_flow(|f, _| f.options.pan_on_scroll = !f.options.pan_on_scroll),
        ))
        .child(button(
            "Drag selects",
            options.selection_on_drag,
            on_flow(|f, _| f.options.selection_on_drag = !f.options.selection_on_drag),
        ))
        .child(button("Save", false, on_root(|r, cx| r.save(cx))))
        .when(has_saved, |d| {
            d.child(button("Restore", false, on_root(|r, cx| r.restore(cx))))
        })
        .into_any_element()
}

/// The bottom panel: what the flow last said.
fn status_line(root: &WeakEntity<Root>, cx: &mut App) -> AnyElement {
    let c = Theme::global(cx).colors;
    let status = root
        .upgrade()
        .map(|r| r.read(cx).status.clone())
        .unwrap_or_default();
    div()
        .px(px(10.))
        .py(px(5.))
        .rounded(px(8.))
        .border_1()
        .border_color(c.border)
        .bg(c.popover)
        .text_size(px(12.))
        .text_color(c.muted_foreground)
        .child(status)
        .into_any_element()
}

impl Root {
    /// One step of the measurement: note when this frame began, move the flow, ask for the
    /// next frame, and at the end print what was seen and quit.
    fn bench_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = &mut self.bench else {
            return;
        };
        let now = Instant::now();
        if let Some(last) = b.last {
            b.intervals
                .push((b.frame, (now - last).as_secs_f32() * 1000.));
        }
        b.last = Some(now);
        b.frame += 1;
        b.started.set((b.frame, Some(now)));
        let frame = b.frame;
        if frame == WARM_UP {
            b.base = self.flow.read(cx).viewport();
        }
        if frame > WARM_UP {
            let t = (frame - WARM_UP) as f32 / 40.;
            let base = b.base;
            let to = if frame <= WARM_UP + PHASE {
                Viewport::new(
                    base.x + 160. * t.sin(),
                    base.y + 100. * (t * 1.3).cos(),
                    base.zoom,
                )
            } else {
                base.zoomed_about(xy(640., 410.), base.zoom * (1. + 0.45 * t.sin()))
            };
            self.flow.update(cx, |f, cx| f.set_viewport(to, cx));
        }
        if frame > WARM_UP + PHASE * 2 {
            let phase = |f: usize| if f <= WARM_UP + PHASE { 0 } else { 1 };
            for (i, name) in ["pan", "zoom"].into_iter().enumerate() {
                let of = |v: &[(usize, f32)]| -> Vec<f32> {
                    v.iter()
                        .filter(|(f, _)| *f > WARM_UP + 1 && phase(*f) == i)
                        .map(|(_, ms)| *ms)
                        .collect()
                };
                report(name, "frame", of(&b.intervals).into_iter());
                report(name, "our work", of(&b.work.borrow()).into_iter());
            }
            cx.quit();
            return;
        }
        window.request_animation_frame();
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.bench_step(window, cx);
        // Painted after the flow, so it marks the end of the frame's work.
        let probe = self.bench.as_ref().map(|b| {
            let (work, started) = (b.work.clone(), b.started.clone());
            canvas(
                |_, _, _| {},
                move |_, _, _, _| {
                    if let (frame, Some(at)) = started.get() {
                        work.borrow_mut()
                            .push((frame, at.elapsed().as_secs_f32() * 1000.));
                    }
                },
            )
            .absolute()
            .size_0()
        });
        div().size_full().child(self.flow.clone()).children(probe)
    }
}

actions!(flow_example, [Quit]);

fn main() {
    let args = args();
    // gpui-core's bundled icons, which nodes' badges draw.
    gpui_platform::application().with_assets(gpui_core::Icons).run(move |cx| {
        gpui_core::init(cx);
        gpui_flow::init(cx);
        apply_theme(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Flow".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // A picture doesn't take the keyboard or come forward, and isn't shown at all.
            focus: args.shot.is_none(),
            show: args.shot.is_none(),
            // GPUI draws a window that isn't the active one at most 30 times a second. The
            // measurement lifts that, so it reads the same whether or not the window is in front.
            inactive_frame_interval: if args.bench {
                None
            } else {
                WindowOptions::default().inactive_frame_interval
            },
            ..Default::default()
        };
        let (window, root) =
            gpui_core::open_window(options, cx, {
                let args = &args;
                move |_, cx| cx.new(|cx| Root::new(args, cx))
            })
            .expect("the window");
        if args.shot.is_none() {
            cx.activate(true);
            let flow = root.read(cx).flow.clone();
            let _ = cx.update_window(window, |_, window, cx| {
                flow.focus_handle(cx).focus(window, cx)
            });
        }
        if args.bench {
            // A window that's covered or on a locked screen gets no frames at all; don't wait
            // for them for ever.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(std::time::Duration::from_secs(30)).await;
                eprintln!("the window wasn't drawn often enough to measure: is it covered, or the screen locked?");
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
        #[cfg(feature = "shots")]
        if let Some(path) = args.shot.clone() {
            shots::shot(window, root.clone(), path, args.scene.clone(), cx);
        }
        let _ = (window, root, &args.scene);
    });
}

#[cfg(feature = "shots")]
mod shots {
    use super::*;
    use std::time::Duration;

    async fn wait(cx: &mut AsyncApp, ms: u64) {
        cx.background_executor()
            .timer(Duration::from_millis(ms))
            .await;
    }

    /// Draws the window now and lets what that drew settle. A window that isn't shown gets no
    /// frames from the display, so a picture draws it itself, more than once: the flow fits
    /// its view from the sizes the frame before measured.
    async fn settle(window: AnyWindowHandle, cx: &mut AsyncApp) {
        for _ in 0..3 {
            let _ = cx.update_window(window, |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            wait(cx, 80).await;
        }
    }

    fn send(window: AnyWindowHandle, cx: &mut AsyncApp, event: PlatformInput) {
        let _ = cx.update_window(window, |_, window, cx| {
            window.dispatch_event(event, cx);
        });
    }

    async fn press(
        window: AnyWindowHandle,
        cx: &mut AsyncApp,
        at: Point<Pixels>,
        modifiers: Modifiers,
    ) {
        send(
            window,
            cx,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at,
                pressed_button: None,
                modifiers,
            }),
        );
        send(
            window,
            cx,
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                modifiers,
                click_count: 1,
                first_mouse: false,
            }),
        );
        settle(window, cx).await;
    }

    async fn drag_to(
        window: AnyWindowHandle,
        cx: &mut AsyncApp,
        at: Point<Pixels>,
        modifiers: Modifiers,
    ) {
        send(
            window,
            cx,
            PlatformInput::MouseMove(MouseMoveEvent {
                position: at,
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }),
        );
        settle(window, cx).await;
    }

    /// A picture of the window, rendered offscreen, then quits.
    pub fn shot(
        window: AnyWindowHandle,
        root: Entity<Root>,
        path: PathBuf,
        scene: String,
        cx: &mut App,
    ) {
        cx.spawn(async move |cx| {
            wait(cx, 500).await;
            settle(window, cx).await;
            let flow = root.read_with(cx, |r, _| r.flow.clone());
            match scene.as_str() {
                // A connection dragged out of School's green handle toward Send, not yet there,
                // and then onto Send's handle in the second half of the name.
                "connect" | "connect-snap" => {
                    let (from, to) = flow.read_with(cx, |f, _| {
                        let from = f
                            .handle_point("school", HandleKind::Source, Some("ok"))
                            .unwrap();
                        let to = f.handle_point("out", HandleKind::Target, None).unwrap();
                        (f.flow_to_screen(from), f.flow_to_screen(to))
                    });
                    press(
                        window,
                        cx,
                        point(from.x - px(2.), from.y),
                        Modifiers::none(),
                    )
                    .await;
                    let end = if scene == "connect" {
                        point(to.x - px(70.), to.y - px(80.))
                    } else {
                        point(to.x - px(9.), to.y + px(6.))
                    };
                    drag_to(window, cx, end, Modifiers::none()).await;
                }
                // A selection rectangle round the two agents on the right.
                "select" => {
                    let (a, b) = flow.read_with(cx, |f, _| {
                        let a = f.node_rect("finances").unwrap();
                        let b = f.node_rect("school").unwrap();
                        let all = a.union(&b);
                        (
                            f.flow_to_screen(xy(all.x - 25., all.y - 25.)),
                            f.flow_to_screen(xy(all.right() + 30., all.bottom() + 20.)),
                        )
                    });
                    press(window, cx, a, Modifiers::shift()).await;
                    drag_to(window, cx, b, Modifiers::shift()).await;
                }
                "zoom" => {
                    let _ = cx.update_window(window, |_, _, cx| {
                        flow.update(cx, |f, cx| {
                            f.options.transition = Duration::ZERO;
                            f.update_node("note", |n| n.selected = true, cx);
                            f.update_edge("triage-school", |e| e.selected = true, cx);
                            f.fit_bounds(Rect::new(-30., 90., 560., 320.), 0.02, cx);
                        })
                    });
                }
                "lines" => {
                    let _ = cx.update_window(window, |_, _, cx| {
                        flow.update(cx, |f, cx| {
                            f.set_background(Some(FlowBackground::lines()), cx);
                            f.set_viewport(Viewport::new(520., 260., 0.45), cx);
                        })
                    });
                }
                _ => {}
            }
            settle(window, cx).await;
            let _ = cx.update_window(window, |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
                match window.render_to_image() {
                    Ok(img) => {
                        if let Err(e) = img.save(&path) {
                            eprintln!("the picture wasn't saved: {e}");
                        }
                    }
                    Err(e) => eprintln!("the window didn't render to a picture: {e}"),
                }
                cx.quit();
            });
        })
        .detach();
    }
}

/// Every name a host is likely to write resolves with both crates imported whole.
#[allow(dead_code, clippy::type_complexity)]
fn names_dont_clash(
    _: (
        Flow,
        Node,
        Edge,
        Handle,
        NodeContext,
        HandleRef,
        Extent,
        LabelAlign,
        NodeShape,
        Palette,
        Side,
        HandleKind,
        EdgeKind,
        Marker,
        Connection,
        Viewport,
        Rect,
        XY,
        FlowBackground,
        BackgroundVariant,
        FlowColors,
        FlowEvent,
        FlowOptions,
        PanelPosition,
        ResizeCorner,
        SelectionMode,
        ConnectionMode,
        FlowObject,
        MarkerKind,
        EdgeDefaults,
    ),
) {
}
