# The JSON form of a flow

gpui-flow stores a flow (a whiteboard or a mind map) as one JSON document: its nodes, its edges
and how it's laid out. Each node, edge and frame has an id that stays the same while the flow
is edited, so an app or a server can store the flow, merge edits to it, and point at its
parts.

This is version 1, written and read by `gpui_flow::json`. `Flow::document()` gives a flow's
document and `Flow::set_document()` loads one; `Document::to_json()` and
`Document::from_json()` write and read the text.

## A document

```json
{
  "format": "gpui-flow",
  "version": 1,
  "layout": { "type": "tree", "direction": "right" },
  "nodes": [
    { "id": "k3W9cQe2LfAp", "position": { "x": 0, "y": 0 }, "type": "topic",
      "data": { "label": "Launch" }, "link": "4f2a" },
    { "id": "Zr81mXbT0qvN", "position": { "x": 220, "y": 0 }, "type": "topic",
      "data": { "label": "Press" } }
  ],
  "edges": [
    { "id": "c7HhQ2wLx5Ke", "source": "k3W9cQe2LfAp", "target": "Zr81mXbT0qvN", "type": "default" }
  ],
  "viewport": { "x": 120, "y": 80, "zoom": 1 }
}
```

| Field | |
|---|---|
| `format` | Always `"gpui-flow"`. |
| `version` | `1`. A reader refuses a higher version. |
| `layout` | How the nodes are laid out. Absent: where they're put, as on a whiteboard. See [Layout](#layout). |
| `nodes` | The nodes, frames included, in drawing order (later is drawn above, before `zIndex`). |
| `edges` | The edges, in drawing order. Absent means none. |
| `viewport` | Where the flow was in view when saved: a flow point `p` shows at `p × zoom + (x, y)`. Optional; a host may keep its own per person instead. |

Any other field is kept as it was and written back. The same goes for nodes, edges and
`data` (see [What a reader keeps](#what-a-reader-keeps)).

## Ids

Every node and every edge has an `id`: 1 to 128 bytes of `A-Z a-z 0-9 - _ . : ~`. Node ids are
unique among nodes and edge ids among edges; a node and an edge may share one. A frame is a node,
so its id is a node id.

An id never changes. Moving, resizing, restyling, relabelling, framing, unframing or
reconnecting something keeps its id. Something new gets a new id: the flow makes 12 characters
from `0-9 A-Z a-z` at random (`json::new_id()`), so two people, or a person and an agent, adding
at the same time don't make the same one. Ids from elsewhere (React Flow, an import) are kept
when they're valid.

On reading, an id that is missing or invalid is replaced, and so is every use of an id after its
first. Edges and `parentId`s that named an invalid node id are pointed at its new id; ones that
named a repeated id keep pointing at its first node.

## A node

```json
{ "id": "db", "position": { "x": 40, "y": 60 }, "type": "shape", "shape": "cylinder",
  "data": { "label": "Postgres" }, "width": 160, "height": 80, "color": "blue",
  "parentId": "storage", "link": "4f2a" }
```

The fields are React Flow's, with this crate's beside them. Only `id` and `position` are
required.

| Field | |
|---|---|
| `id` | See [Ids](#ids). |
| `position` | `{ "x", "y" }`: the top left corner, in the flow, or from the parent's top left corner when it has a `parentId`. |
| `type` | The node's kind (below). `"default"` when absent. |
| `data` | What the kind shows. The built-in kinds read `data.label`, the node's text. Anything else in it is kept. |
| `width`, `height` | A fixed size. Without one, the node is as large as its content. |
| `sourcePosition`, `targetPosition` | `"top"`, `"right"`, `"bottom"` or `"left"`: the side the built-in kinds put their source and target handle on. `"bottom"` and `"top"` by default. |
| `parentId` | The frame or group the node is in. It moves with its parent and is drawn above it. |
| `extent` | `"parent"` to keep the node inside its parent, or `[[x1, y1], [x2, y2]]` to keep it inside that box of the flow. |
| `zIndex` | Higher is drawn later. 0 by default. |
| `hidden` | Not drawn. |
| `draggable`, `selectable`, `connectable`, `deletable` | Override the flow's own switches for this node. |
| `collapsed` | In a mind map, what's under the node is folded away. |
| `shape` | A `shape` node's outline: `rectangle`, `rounded`, `ellipse`, `diamond`, `hexagon`, `cylinder`, `parallelogram`, `document` or `cloud`. |
| `color` | A palette colour by name: `red`, `orange`, `yellow`, `green`, `teal`, `blue`, `purple` or `pink`. The kind's own colours when absent or `default`. |
| `icon` | A [Lucide](https://lucide.dev) icon by name (`database`), shown as a badge. |
| `emoji` | One emoji, shown as a badge. |
| `textAlign` | `left`, `center` or `right`. |
| `media` | An `image` node's picture: `{ "src", "width", "height", "alt" }`. See [Pictures and cards](#pictures-and-cards). |
| `card` | A `link` node's card: `{ "url", "title", "description", "site", "image" }`. |
| `link` | Another document this node links to, by the document's id. See [Links](#links-to-other-documents). |

`selected` is never stored (see [Not stored](#not-stored)).

### Kinds

| `type` | What it is |
|---|---|
| `default` | A box with its label, a target handle and a source handle. |
| `input`, `output` | The same with only a source, or only a target. |
| `group` | A plain area that holds child nodes. |
| `frame` | A titled area on a whiteboard (below). |
| `shape` | A whiteboard shape, outlined as `shape` says. |
| `topic` | A mind-map topic. |
| `text` | A heading: text with no box. |
| `image` | A picture, with its label as the caption. |
| `link` | A card for a web address. |
| `drawing` | A pen line, `data.points` being `"x,y x,y …"` with each value from 0 to 1 across the node's box. |

A host can draw kinds of its own. A kind a reader doesn't know is kept, and drawn as a `default`
node.

### Frames

A frame is a node of type `frame`, titled with `data.label`. A node in it has the frame's id as
its `parentId`, and its `position` is from the frame's top left corner. Frames can hold frames.

Parents may come before or after their children in `nodes`; a reader takes either. A
`parentId` that names no node, or that loops back round, is drawn as if absent and kept as it
is.

## An edge

```json
{ "id": "api-db", "source": "api", "target": "db", "type": "smoothstep", "label": "reads",
  "markerEnd": { "type": "arrowclosed" }, "style": { "stroke": "#4060ff", "strokeDasharray": "6 4" } }
```

| Field | |
|---|---|
| `id` | See [Ids](#ids). |
| `source`, `target` | The ids of the nodes it runs from and to. |
| `sourceHandle`, `targetHandle` | Which handle, on a node with several. |
| `type` | How the line runs: `default` (a curve), `straight`, `step`, `smoothstep` or `simplebezier`. Any other name is kept and drawn as `default`. |
| `label` | Text along the line. |
| `markerStart`, `markerEnd` | An arrowhead: `{ "type": "arrow" \| "arrowclosed", "color", "width", "height" }`. |
| `style` | `stroke` (a `#rrggbb` colour), `strokeWidth`, and `strokeDasharray`: dotted when the dashes are much shorter than the gaps, dashed otherwise, solid when absent. The rest of `style` is kept. |
| `color` | A palette colour by name, for the line, its arrowheads and its label. An exact `style.stroke` wins over it. |
| `animated` | Dashes run from the source to the target. |
| `hidden`, `selectable`, `deletable`, `zIndex`, `interactionWidth` | As React Flow has them. |
| `data` | Anything of the host's; kept. |

An edge whose `source` or `target` names no node is kept and not drawn.

## Layout

| `layout` | |
|---|---|
| absent | A whiteboard: nodes are where their `position` puts them. |
| `{ "type": "tree", "direction": "right" \| "down" \| "both" }` | A mind map. The tree is read from the edges: a node's parent is the source of the first edge into it, and its children are in the order of their edges. Positions of nodes under a root are worked out on load, and stored as last laid out. |

A layout of another type is kept as it was, and the flow lays the nodes out freely.

## Links to other documents

A node links to another document by that document's id, in its `link`. The flow shows a link
button on the node and tells the host when it's pressed; it opens nothing itself.

To find every link without this crate, take `link` from each node. `Document::links()` does
the same in Rust, once each, in the order of the nodes.

## Pictures and cards

A picture's `media.src` and a card's `card.image` are whatever the host means by them, such as
a file id or an address. The flow asks the host's image loader for them, and never fetches
anything itself. A card's `title`, `description` and `site` are what the host found at its
`url`; the flow asks it once when they're missing.

## Not stored

`selected` belongs to each person's own session. `Document::to_json()` leaves it out, a read
document has nothing selected, and `Flow::set_document()` keeps what was selected where it's
still there.

## What a reader keeps

A field it doesn't know, on the document, a node, an edge, a `media` or `card`, or in `data`
and `style`, is kept and written back unchanged. The exceptions are an arrowhead's object and
`viewport`, which keep only the fields above, and an arrowhead given by name (a React Flow SVG
id), which reads as none. So is a node `type`, edge `type` or `layout` it
doesn't know. A newer writer can add fields and kinds to version 1 without a reader losing
them. The version goes up only for a change a version-1 reader would get wrong, such as a field
whose meaning changes.

## Reading JSON

`Document::from_json()` refuses text that isn't JSON, a `format` other than `"gpui-flow"`, and a
higher `version`. Otherwise it fixes ids as [Ids](#ids) says. On load, the flow treats a position
or size that isn't a finite number as 0 or absent. Nothing in a document can make the flow fetch,
open or run anything.

## Edits

The flow tells its host what changed as a batch of operations by id
(`FlowEvent::Edits`), and takes batches from elsewhere with `Flow::apply()`. A server stores a
flow by applying each batch to its document in order (`Document::apply()` in Rust), so two
people, or a person and an agent, editing different things never overwrite each other.

```json
[
  { "op": "addNode", "node": { "id": "Zr81mXbT0qvN", "position": { "x": 220, "y": 0 }, "data": { "label": "Press" } } },
  { "op": "addEdge", "edge": { "id": "c7HhQ2wLx5Ke", "source": "k3W9cQe2LfAp", "target": "Zr81mXbT0qvN" } },
  { "op": "patchNode", "id": "k3W9cQe2LfAp", "patch": { "position": { "x": 40 }, "data": { "label": "Launch!" } } },
  { "op": "removeEdge", "id": "e1" },
  { "op": "removeNode", "id": "n7" }
]
```

| `op` | Fields | Applied |
|---|---|---|
| `addNode` | `node`: the whole node, as stored | Added at the end of `nodes`. One with the same id is replaced, in its place. |
| `patchNode` | `id`, `patch` | The patch is merged into the node's stored form (below). |
| `removeNode` | `id` | The node is taken out. |
| `addEdge`, `patchEdge`, `removeEdge` | The same, with `edge` | The same, on `edges`. |

A frame is a node, so frames change by the node operations.

**A patch** is a JSON merge patch ([RFC 7386](https://www.rfc-editor.org/rfc/rfc7386)) of the
stored form, holding only what changed: an object is merged field by field (so moving a node
sends `{"position": {"x": 40}}` when only `x` changed, and relabelling it
`{"data": {"label": "Launch!"}}`), anything else (a string, a number, an array) replaces the
field, and `null` takes the field away. A patch never changes an `id` or `selected`; a reader
drops those from it. If a patched node or edge no longer reads (a field of the wrong type), the
patch is skipped.

**A removal takes only what it names.** When this crate deletes a node, the same batch also
removes every edge that touched it, and removes or unparents (with a `patchNode` of
`"parentId": null` and its new `position`) each node inside it. A reader applies
operations literally and doesn't cascade: an edge or `parentId` left naming a removed node
(when, say, someone connected to it while someone else deleted it) is kept, and drawn as if
absent. Undoing the removal brings the node back, and that edge with it.

**What isn't there:** a patch or removal of an id that isn't there is skipped; an add of an id
that is replaces it; an add with an invalid id is skipped.

**Batches.** One batch is one edit, sent when it settles: a drag of many nodes, by mouse or
finger, is one batch at its end; making a frame is the frame's `addNode` and each child's
`patchNode`; deleting is every removal at once. Within a batch, adds come first (nodes, then
edges), then patches, then removals (edges, then nodes). Edits made through the flow's methods
(`update_node`, `set_nodes`, `connect`, …) are sent too. Loading (`set_document`, `restore`)
and applying (`apply`) send nothing, and neither do selecting, panning and zooming.

**In a mind map,** a node's place under its root is the layout's, worked out on each device, so
it isn't sent: only a root's `position` is. Adding a child is its `addNode` and the `addEdge`
from its parent.

## React Flow

Without `format`, `version` and `layout`, a document is React Flow's own `{ nodes, edges,
viewport }`, which `Flow::to_object()` and `Flow::restore()` write and read. A React Flow app
can show a gpui-flow whiteboard's nodes and edges, though not this crate's own kinds and fields.
