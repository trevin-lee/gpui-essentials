# gpui-flow

A node-and-edge editor for [GPUI](https://www.gpui.rs) apps in the manner of
[React Flow](https://reactflow.dev): a library, and an example window with one of everything.
It's young: the API can change between commits, so pin one.

## Use it

It needs Rust 1.100 or newer. It uses GPUI as `gpui-pre = "=0.3.12"` and takes its colours
from gpui-core's theme, so your app must use the same GPUI, or two would be linked:

```toml
[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.12" }
gpui-core = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
gpui-flow = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
```

Call `gpui_core::init(cx)` and then `gpui_flow::init(cx)` once at startup.

Run the example with `cargo run --release --example flow` from this folder.

## How it works

- **What it is.** A `Flow` is a GPUI entity that holds nodes, edges and a viewport, and emits
  events. It carries React Flow's pieces under their names: the `default`, `input`, `output`
  and `group` node kinds and the host's own; handles; bezier, simple bezier, straight, step and
  smooth step edges (the same routes, ported, up to 12.12.1's corner fix) with labels,
  arrowheads, colors and animation; the dotted, lined or crossed background; the controls; the
  minimap; panels.
- **React Flow's behaviour, checked against its source.** The defaults are React Flow's (zoom
  limits, grid, connection radius in flow units, wheel and scroll speeds, fit padding,
  `setCenter` zooming to the largest), except two kept for speed: nodes off screen aren't
  drawn, and nodes too small to read are drawn as boxes. So are its rules: a plain click on a
  selected node keeps the selection and ⌘ adds or takes away; shift-drag draws a rectangle
  even from a node, and takes the edges of any node it takes; a dragged connection snaps to a
  handle under the pointer, else the nearest in reach, preferring the other kind; grid snapping
  is in the flow, not the parent; `extent` holds on load as well as in a drag; a child is
  always drawn above its parent, selected nodes above the rest, and an edge to a child at that
  child's level (so above the group it's in); an edge whose handle doesn't exist isn't drawn;
  delete takes the children that may go, and leaves one that may not where it is.
- **React Flow's JSON.** A flow saves and loads as React Flow's own `toObject()` JSON:
  camelCase fields, `parentId`, `extent` (`"parent"` or a box), `zIndex`, handles, markers,
  `style.stroke` and `strokeWidth`, custom edge types by name (drawn as bezier), and any field
  it doesn't read kept as it was, so a file comes back without loss.
- **With the mouse and keys.** Drag the pane to pan; scroll or pinch to zoom about the pointer;
  drag a node, or all that are selected; ⌘-click to add or take away; shift-drag for a
  rectangle; drag from a handle to another (either direction) to connect; click an edge to
  select it; delete, ⌘A, escape (which also puts back a drag, resize, pan or rectangle), the
  arrows (shift for four steps), ⌘+, ⌘−, ⌘0, ⇧1 to fit. The keys sit on a layer of their own
  beside the nodes, so they only act while the flow itself has the focus, never while a text
  field in a node or panel does, and a key with nothing to do goes on to the host.
- **Inside a node.** A child that takes the focus when pressed (a text field) owns that press:
  the node isn't selected or dragged, and the field keeps the focus. Others are marked as React
  Flow marks them: `nodrag` for buttons and sliders, `nowheel` for lists that scroll.
  Renderers get the `&Flow` to read (the entity is busy while it draws).
- **Events a host can follow.** The events say what someone did: connect (with where a
  dropped connection ended, for "drop on the pane to add a node"), clicks, drag start, move
  and stop (escape included), arrow-key moves, resize start, move and end, delete (once, and
  a host check can refuse it), and the selection and viewport whatever changed them,
  transitions included. Changes made through the flow's methods aren't reported back.
- **A view over the canvas.** A host can lay a native view of its own over the canvas, such
  as a PencilKit canvas for drawing on a mind map with a pencil, and keep it lined up.
  `with_transform_listener` hands over the canvas's `CanvasTransform` (the pane's bounds in the
  window, and the pan and zoom) on the first frame and on every frame it changes, including
  each step of an animated zoom and the pane moving in the window. It's called while that
  frame is drawn, so the view moves in the same frame as the canvas. `canvas_transform()`
  reads it at any time. `set_pointer_mode` says who takes the pointer:
  - `Edit`, the usual;
  - `Navigate`: a drag anywhere only pans, and scrolling and pinching zoom, so fingers move
    the canvas while the pen draws;
  - `Off`: the canvas takes no presses, scrolling or pinches at all.
- **Touch.** On iPhone and iPad, GPUI turns fingers into gestures and the flow takes them as
  React Flow does on a touchscreen, with Apple's sizes.
  - A finger on a node drags it, and a tap selects it; two taps make a double tap.
  - A finger on a handle drags out a connection, and one on a resizer's grip resizes.
  - Anywhere else a finger pans, in any direction, and a tap on an edge selects it.
  - Held still for half a second, a finger asks for a context menu (`FlowEvent::ContextMenu`,
    for a node, an edge or the pane, as a right click does on a Mac). The host shows the
    menu.
  - Handles and grips take a finger from a 44-point area (`touch_target_size`) that grows
    outward but not into the node, and edges take one from 22 points away.
  - A `nodrag` child, such as a button, gets an ordinary tap.
  - GPUI doesn't recognise a two-finger pinch on iOS yet, so the host recognises it natively
    and calls `zoom_by` (or dispatches `PinchEvent`s).
- **Mind maps.** With `layout: Layout::Tree(TreeLayout { .. })` a flow is a mind map. Its
  tree is read from the edges: a node's parent is the source of the first edge into it, so
  a mind map saves and loads as any flow does.
  - **Layout.** Roots stay where they're put. Everything under them is laid out from the
    nodes' sizes: to the right, downward, or to both sides of the root (`TreeDirection`),
    with siblings in edge order.
  - **Motion.** Nodes ease to their new places, and nothing redraws once they're there.
  - **Edges** join the sides of the boxes.
  - **Chips.** A chip on each parent folds its subtree away, showing a count, or shows it
    again (`Node::collapsed`, `FlowEvent::NodeCollapse`).
  - **Tab** adds a child to the selected node and **enter** a sibling after it. The new
    node is selected, and `FlowEvent::NodeAdd` tells the host to start editing it
    (`with_new_node` makes nodes your way).
  - **Arrows** move the selection to the nearest node that way.
  - **Moving.** A node dropped on another becomes its child, with the target outlined
    while you aim (`FlowEvent::NodeReparent`). Dropped on nothing, it goes back.
  - **Delete** takes a node's subtree and selects its parent.
  - **Topics.** The built-in `topic` kind is a label without handles, larger for a root.
  - The example has one: `--mindmap`.
- **Links.** A node can link to something of the host's by id (`Node::link`, saved as
  `"link"`), such as a document in Docs. The built-in kinds then show a small ↗ button
  after their label, and `NodeContext::link_button()` puts the same button into a kind of
  your own. Pressing it, or ⌘↩ with the node selected, sends `FlowEvent::OpenLink`. The
  host decides what the id means and opens it; the flow never opens or follows anything
  itself, so a hostile file can't make it.
- **Nodes are ordinary elements.** A renderer per kind returns any GPUI element. GPUI has no
  transform for elements, so a node can't be scaled the way a CSS-transformed one is: the flow
  sets the rem under each node to 16 points times the zoom, and nodes size themselves in
  `units` (rems underneath), and their text with `font_size`. What a node sizes in pixels, a
  hairline border, stays that size at every zoom. Nodes wholly off screen aren't built once
  they've been measured; one that changes is built again so its handles stay true.
- **What the flow paints itself.** Edges, the background, the minimap and the controls' icons,
  with GPUI's paths and quads. Each node and handle reports where it was laid out, and the
  edges are painted after that in the same frame, so a line never trails its node. Edges are
  painted in levels among the nodes, cut to the pane (an edge running far off screen costs
  only what's on it), and hit-tested against the lines last painted.
- **Bad input.** Positions and sizes that aren't numbers, zoom limits the wrong way round, a
  grid of 0, a parent chain that loops, two nodes with one id: each is put right or ignored,
  never a panic or a hang, and each has a test.
- **What a frame costs.** Measured by `--bench` in the example (a release build, 1280×820 at
  2×, 120 Hz), as the app's own work from the start of a frame to the end of its painting:
  0.5 ms panning the sample and 1.2 ms zooming it, so both hold 120 frames a second; 2.3 ms
  with 500 nodes and 4 ms with 1000, zoomed out to fit, where they're drawn as boxes. What got
  it there, each found with a profiler:
  - The background's marks and the edges are each gathered into one path. GPUI gives every
    primitive a place in its ordering tree and every batch of paths its own render pass, so
    ten thousand dots as quads and an edge a path cost milliseconds each.
  - Text takes its size from `NodeContext::font_size`, which snaps to half a pixel. Text that
    follows the zoom exactly asks for a new font size on every frame of a zoom, and making the
    font and its glyphs then costs more than the rest of the frame.
  - Nodes are looked up and changed by id through an index, not by searching the list.
  - Below a zoom of 0.3, nodes are drawn as boxes of the size they measured, not built.
  - Edges off screen are skipped before they're flattened.
- **Drawing only on change.** Nothing draws while nothing changes, with one exception:
  animated edges redraw the flow twenty times a second while one is on screen.
- **Names.** React Flow's `Position` is `Side` and `Background` is `FlowBackground`, because
  GPUI exports both names and hosts import both crates whole.
- **Not yet.** Reconnecting an existing edge by dragging its end; a node toolbar; edge
  renderers of the host's own (a custom type is drawn as a bezier); panning when a drag nears
  the pane's edge; connecting by clicking two handles in turn; `dragHandle`; markers that
  scale with the line's width; a keyboard path through nodes and accessibility; undo.

## License

Apache-2.0. See the repository's LICENSE and NOTICE.
