# The JSON form of a document

gpui-blocks keeps a document as Markdown while it's edited, and can also give it as JSON: a
tree of blocks, each with an id. Store the JSON when you need to know which block is which
across saves, for example to keep links, comments or search results pointing at a block.
Markdown stays the way in from other tools and the way out to them.

This is version 1, written by `gpui_blocks::json`.

## A document

```json
{
  "format": "gpui-blocks",
  "version": 1,
  "front_matter": "---\ntitle: Weekly review\n---\n",
  "blocks": [ … ]
}
```

| Field | |
|---|---|
| `format` | Always `"gpui-blocks"`. |
| `version` | `1`. A reader refuses a higher version, which may hold blocks it can't write back. |
| `front_matter` | YAML front matter with its `---` lines, or absent. Kept as it was, never parsed. |
| `blocks` | The top-level blocks, in order. |

## A block

```json
{ "id": "Zr81mXbT0qvN", "type": "paragraph", "text": "The plan is in [Roadmap](doc:4f2a)." }
```

Every block has an `id` and a `type`. Blocks that hold other blocks have `children`, omitted
when empty. The other fields depend on the type.

**Ids** are 1 to 64 of `A-Z a-z 0-9 - _`, unique in the document. The editor makes new ones
of 12 characters. A block keeps its id while it's edited and moved. A block made by splitting
or pasting gets a new one, and so does every block after `set_markdown`. When JSON is loaded,
a block keeps the id it was stored with. A missing, invalid or repeated id is replaced, and so
is the id of a block that reads back as something else (see [Reading JSON](#reading-json)).

**Text** is inline Markdown on one line, as the editor writes it: `**bold**`, `*italic*`,
`~~strike~~`, `` `code` ``, `<u>underline</u>`, `<mark>highlight</mark>`, `[link](url)`,
`![picture](url)`, a hard break as `<br>`, and mentions of other documents (below). A newline in
stored text is read as a space.

## Types

| `type` | Fields | `children` |
|---|---|---|
| `paragraph` | `text` | |
| `heading` | `level` (1 to 6), `text` | |
| `quote` | | any blocks |
| `callout` | `name`: GitHub's alert name in lowercase (`note`, `tip`, `important`, `warning`, `caution`) | any blocks |
| `toggle` | `open` | the summary (a `paragraph`), then the body |
| `list` | `ordered`, `start` (an ordered list's first number, absent when it's 1) | `item`s |
| `item` | `checked` (`true` or `false` for a to-do item, absent otherwise), `spread` (`true` for an item written with blank lines between its blocks) | any blocks |
| `code` | `language`, `meta` (the rest of the info string), `code` | |
| `divider` | | |
| `table` | `align`: per column, `"left"`, `"center"`, `"right"` or `null` | `row`s, the first one the header |
| `row` | | `cell`s |
| `cell` | `text` | |
| `toc` | | |
| `embed` | `doc`: the document's id; `title`: its title when the embed was made; `view`: how the app shows it (`table`, `board`, `mindmap`, `calendar`, `page`, or another 1 to 24 of `a-z 0-9 -`); `height`: in points, 80 to 1600 | |
| `page` | `doc`: the document's id; `title`: its title when the link was written | |
| `widget` | `name`, `meta` (the rest of the info string), `body` | |
| `markdown` | `markdown`: the block as Markdown | |

A `markdown` block is what the editor keeps as is: a link definition, an HTML block, and in
later versions any type a reader doesn't know.

## Links to other documents

A document links to another in two ways, both by the other document's id:

- a **mention**, inside any `text`: `[Roadmap](doc:4f2a)`;
- a **sub-page**, a block: `{ "type": "page", "doc": "9c1e", "title": "Last week" }`;
- an **embed**, a block that shows the other document inside this one: `{ "type": "embed",
  "doc": "b1", "title": "Q4 board", "view": "board", "height": 480 }`.

A document id is 1 to 128 of `A-Z a-z 0-9 - _ . ~`. To find every link without this crate,
walk the blocks: take `doc` from each `page` and `embed`, and from every `text` each match of

```
\]\(doc:([A-Za-z0-9._~-]{1,128})\)
```

`Document::links()` and `Document::pages()` do the same in Rust. The titles in the JSON are
only what was shown when the link was made; the editor draws each link with the title the app
gives it now.

## Reading JSON

JSON is read by writing it as Markdown and parsing that, then giving the parsed blocks the ids
of the JSON blocks, matching the two trees block by block. JSON written by this crate always
reads back as the same blocks. JSON from elsewhere can't break out of a block: newlines in
text, backticks in a code block's info string and odd callout names are neutralised, though a
block can come back as another type (a callout with an unknown name is a quote), and then gets
a new id.

## Markdown

`Document::from_markdown` and `Document::to_markdown` convert without an editor, for importing
and exporting. Markdown has no ids, so `from_markdown` makes new ones every time.

## Merging two saves

When a save is refused because someone else saved first, merge the two edits and save the
result. `gpui_blocks::json::merge(base, ours, theirs)` does a three-way merge by block id, where
`base` is the revision both edits started from:

- A change one side made to a block the other left alone applies: an edit, a move, a deletion,
  or a new block, which keeps its place after the block it followed.
- Both edited the text of the same paragraph, heading, cell or code block: the edits merge
  when they don't overlap (by character). When they do, ours is kept.
- One side deleted a block the other edited: the edit is kept. One side deleted a block the
  other put something new inside: the block is kept for it.
- Both moved a block to different places: ours is kept.

It returns the merged document and a list of conflicts, one for each change of theirs it set
aside and each deletion it overrode, so nothing is lost without a report:

| `type` | Fields | |
|---|---|---|
| `content` | `id`, `theirs` (their version of the block) | Both changed the block's content in ways that overlap |
| `deleted_changed` | `id`, `deleted_by` (`ours` or `theirs`) | The change was kept |
| `restored` | `id`, `deleted_by` | Kept because something stays inside it |
| `moved` | `id`, `parent`, `after` (where theirs put it) | Both moved it |
| `front_matter` | `theirs` | Both changed the front matter |

Merging with an unchanged side gives the other side. Apart from which side wins a conflict,
swapping `ours` and `theirs` gives the same document. Blocks put at the same place by both
sides go in id order.

## Operations

`diff(a, b)` gives the changes from `a` to `b` as a list of operations, and `apply(a, ops)`
makes them; `apply(a, diff(a, b))` is `b`. They're for sending edits rather than whole pages.

```json
[
  { "op": "insert", "id": "n1", "type": "paragraph", "text": "New", "parent": null, "after": "a" },
  { "op": "move", "id": "b", "parent": "q", "after": null },
  { "op": "update", "id": "c", "type": "heading", "level": 2, "text": "Renamed" },
  { "op": "delete", "id": "d" },
  { "op": "front_matter", "front_matter": "---\ntitle: x\n---\n" }
]
```

- `insert` adds one block, without children, in `parent` (`null` for the top level) after the
  block `after` (`null` for first). Its children come as their own inserts.
- `move` takes a block and everything in it to `parent` after `after`.
- `update` replaces a block's content (its fields besides `id` and `children`).
- `delete` removes a block and everything in it.

`apply` refuses an operation on a missing block, an insert of an id already there, an `after`
that isn't in that parent, and a move into the block itself, and then changes nothing.
