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
| `callout` | `name`: GitHub's alert name in lowercase (`note`, `tip`, `important`, `warning`, `caution`); optional `icon`: one emoji; optional `color`: `gray`, `brown`, `orange`, `yellow`, `green`, `blue`, `purple`, `pink` or `red` | any blocks |
| `toggle` | `open` | the summary (a `paragraph`, or a `heading` of level 1 to 3 for a toggle heading), then the body |
| `list` | `ordered`, `start` (an ordered list's first number, absent when it's 1) | `item`s |
| `item` | `checked` (`true` or `false` for a to-do item, absent otherwise), `spread` (`true` for an item written with blank lines between its blocks) | any blocks |
| `code` | `language`, `meta` (the rest of the info string), `code` | |
| `divider` | | |
| `table` | `align`: per column, `"left"`, `"center"`, `"right"` or `null` | `row`s, the first one the header |
| `row` | | `cell`s |
| `cell` | `text` | |
| `toc` | | |
| `embed` | `doc`: the document's id; `title`: its title when the embed was made; `view`: how the app shows it (`table`, `board`, `mindmap`, `calendar`, `page`, or another 1 to 64 of `A-Z a-z 0-9 - _`, such as a database view's id); `height`: in points, 80 to 1600 | |
| `page` | `doc`: the document's id; `title`: its title when the link was written | |
| `bookmark` | `url`: the page's `http` or `https` address, at most 2048 bytes, with no spaces, quotes, angle brackets or backslashes; optional `title` and `description` (each at most 300 characters written) and `image` (an `asset:<sha256>` the server stored) | |
| `widget` | `name`, `meta` (the rest of the info string), `body` | |
| `columns` | | 2 to 5 `column`s. Only at the top level |
| `column` | `width`: its share of the row, in (0, 1]; the shares add up to 1 | any blocks but `columns` |
| `markdown` | `markdown`: the block as Markdown | |

Columns are blocks side by side. The editor keeps them in that shape after every edit:
- an emptied column goes;
- a columns block left with one column becomes that column's blocks;
- columns never nest, and never sit below the top level.

Widths are written as shares of 1 to 3 decimals. On reading, a missing width is an even share,
and a row is rescaled only when its widths add up to more than 0.01 away from 1. In Markdown,
columns are HTML comments around their blocks, which other tools don't show:

```markdown
<!-- columns -->

<!-- column 0.5 -->

On the left.

<!-- column 0.5 -->

On the right.

<!-- /columns -->
```

A bare `<!-- column -->` takes an even share. Markers that don't make a whole columns block at
the top level are read as the HTML they are, and nothing between them is lost: for example a
column outside columns, a missing `<!-- /columns -->`, nesting, a bad width, or one or more
than five columns.

A callout's `icon` and `color` are its own look over its kind's, which it keeps. The icon is
one grapheme of at most 16 bytes, with no space or control character and not an `asset:`. In
Markdown they're a comment on the marker's line, either key optional:
`> [!TIP] <!-- icon:💡 color:yellow -->`. A comment that isn't exactly that (another key, an
invalid value, more after it) stays as the callout's first paragraph. An invalid icon or
colour in JSON is read as none.

A bookmark is a web page as a card. Its Markdown is a link titled `bookmark` alone on its line,
its text the page's title (or the address when there's none), then, when it has a description
or picture, a comment holding them as a JSON object, either key optional:
`[Title](https://example.com/a "bookmark") <!-- {"description":"…","image":"asset:<sha256>"} -->`.
In the JSON, `<`, `>` and `&` are written as `\u003c`, `\u003e` and `\u0026`, so it can't end the
comment. A comment that isn't exactly that (another key, a value that isn't a string, not
JSON) makes the line an ordinary paragraph. Only an `asset:` picture is written or drawn:
another is dropped, and the editor writes only a picture an unfurl gave it. A title or
description is cut to 300 characters when written, and read whole. A title that's empty or the
address itself reads back as none. An address the server wouldn't take is never made a
bookmark: it stays a plain link.

A `markdown` block is what the editor keeps as is: a link definition, an HTML block, and in
later versions any type a reader doesn't know.

A toggle heading's Markdown puts the level around the summary: `<details><summary><h2>Title</h2></summary>…</details>`.
Tags are read in either case. A summary with mismatched tags, or `<h4>` to `<h6>`, stays a
paragraph summary, as written.

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

### Types from a later version

A later version may add block types within version 1. This version never refuses a document
for one:
- A block of a type it doesn't know that has blocks inside is read as those blocks, in its place
  and in order, at any depth. For example, an older version reads a `columns` block as its
  columns' blocks, one after another.
- One with nothing inside is kept as a `markdown` block: its own `markdown` or `text` if it has
  one, else a comment naming it (`<!-- a sparkle block, from a newer version -->`).

Nothing is lost from view. Saving writes the reduced form, so a document with newer blocks should
be edited with the version that wrote them. A block of a known type that's missing what it needs
is still refused.

## Markdown

`Document::from_markdown` and `Document::to_markdown` convert without an editor, for importing
and exporting. Markdown has no ids, so `from_markdown` makes new ones every time.

The server's Markdown export writes each block's id in a comment line before it
(`<!-- id:… -->`, inside a toggle too, with the title's id between `<details>` and
`<summary>`). gpui-blocks writes no id lines and doesn't read them back: its ids live only in
the JSON. Read from Markdown, id lines are HTML like any other, not ids.

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
