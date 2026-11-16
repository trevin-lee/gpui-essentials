# Limit vectors

Cases for checking stored flows against gpui-flow's `Limits` (the defaults, as
[docs/json.md](../../../docs/json.md#limits) lists them). The crate's `tests/limits.rs` runs every
case, and the server runs the same files against its own checks. Both must give exactly the
listed problems.

- `valid.json`: documents with no problems, including every text, address and id at its limit,
  a node 16 frames deep, and the cases a reader must tolerate (a loop of parents, an edge to a
  node that isn't there, unknown fields).
- `invalid.json`: documents that each break one limit or check (one breaks two), with the
  problems they must give.
- `batches.json`: batches of edits, checked on their own (`Limits::check_batch`).

## The format

Each file is a JSON array of cases, one per line:

```json
{"name":"a label of 10,001 characters","doc":{...},"problems":["nodes[0].data.label: 10001 characters is more than 10000"]}
{"name":"4 nodes","limits":{"nodes":3},"doc":{...},"problems":["nodes: 4 is more than 3"]}
{"name":"3 ops","limits":{"ops":2},"ops":[...],"problems":["ops: 3 is more than 2"]}
```

- `name` says what the case is.
- `limits`, when present, lowers some limits for this case only, in `Limits`' camelCase names
  (`nodes`, `edges`, `ops`, `idChars`, `textChars`, `urlChars`, `depth`, `nodeBytes`,
  `edgeBytes`); the rest are the defaults. Counts and sizes are checked this way, so no case
  needs 10,000 nodes or a megabyte of text.
- `doc` is a document to check as it's stored: read it without mending ids. `ops` is a batch.
- `problems` lists every problem, in any order (compare them sorted). Each is a path, `: `, then
  a message.

## Paths

A path starts at the document's root, in the JSON's own names: `nodes`, `nodes[3].id`,
`nodes[3].data.label`, `nodes[3].media.alt`, `nodes[3].card.title`, `nodes[3].parentId`,
`edges[2].sourceHandle`, and `nodes[3]` or `edges[2]` for a whole one. In a batch: `ops`,
`ops[1].node.data.label`, `ops[1].edge.label`, `ops[1].id`, `ops[1].patch`. Indexes count from
0.

## Messages

N is the number found and L the limit. Strings count Unicode characters, not bytes.

| Message | When |
|---|---|
| `N is more than L` | Too many nodes, edges or ops |
| `N characters is more than L` | A string too long: an id, an edge's `source`, `target` or handle, a `parentId`, a text, an address |
| `not a valid id` | An id that's empty or has a character other than `A-Z a-z 0-9 - _ . : ~` (and isn't too long) |
| `<id> is used twice` | A node or edge id used before in the same list: given for each use after the first |
| `N levels is more than L` | A node inside more frames than L, counted up its `parentId`s until a parent that isn't there or one met again |
| `more than L bytes` | A node, edge or patch whose compact JSON is larger than L. The size isn't given, since serializers differ by a few bytes (`1` or `1.0`); the cases stay well away from the limit |

The cases were written by a one-off script, and are kept as data, not regenerated. When a limit or
a check changes, edit the cases; the crate's tests say which no longer match.
