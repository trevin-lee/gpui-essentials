# gpui-tables

Notion-style databases for [GPUI](https://www.gpui.rs) apps: tables, boards and lists over rows
your app owns.

A database here is a doc with typed properties (its columns) and saved views. Each of its rows is
a doc too: a page your app opens with its own editor. The crate reads the docs as JSON, works out
what each view shows (filter, sort, group), and hands every change back to your app as an edit.
It never stores anything, fetches anything or navigates.

It's being built for a personal CRM in a Notion-like docs app: People, Companies and Interactions
databases related to each other, an agent or two logging calls and follow-ups, and every person
and company a page of notes.

**Status: the model and engine.** This first version is the JSON model, the engine that filters,
sorts and groups, fractional-index ordering, and the limits on what a database may hold. None of
it needs GPUI. The views (table first, then board and list) come next, behind the default `gpui`
feature, which is empty for now. This README is the design for all of it.

## Contents

- [Use it](#use-it)
- [Databases and rows are docs](#databases-and-rows-are-docs)
- [Properties and values](#properties-and-values)
- [Views](#views)
- [Filters](#filters)
- [Edits and events](#edits-and-events)
- [Relations](#relations)
- [Order](#order)
- [Limits](#limits)
- [The host's part](#the-hosts-part)
- [Performance](#performance)
- [iOS and touch](#ios-and-touch)
- [Not in v1](#not-in-v1)
- [Tests](#tests)

## Use it

It needs Rust 1.100 or newer.

```toml
[dependencies]
gpui-tables = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
```

Pin a commit: until it's on crates.io, any commit can change the API. When the views land they'll
draw through the workspace's pinned GPUI, and your app must use the same one.

A crate that only reads and checks databases (a server, a sync layer, an agent's tool) turns the
views off and gets no GPUI at all:

```toml
gpui-tables = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>", default-features = false }
```

Reading a database and working out a view:

```rust
use gpui_tables::engine::{query, Context, GroupKey};
use gpui_tables::Doc;

let database: Doc = serde_json::from_str(r#"{
    "id": "7d1c2f0e-4b8a-4e55-9d3c-2a6b1f0e9c11", "kind": "database", "title": "People",
    "order": "a0", "folderId": null, "parentDocId": null, "parentObjectId": null,
    "cover": null, "favorite": false, "agents": [], "author": {"kind": "owner"},
    "updatedBy": {"kind": "owner"}, "created": 1793000000000, "updated": 1793000000000,
    "rev": 1, "seq": 10, "deleted": false,
    "database": {
        "properties": [
            {"id": "name", "name": "Name", "type": "title", "config": {}, "order": "a0"},
            {"id": "stage", "name": "Stage", "type": "select", "order": "a1", "config": {
                "options": [{"id": "lead", "name": "Lead", "color": "gray"},
                            {"id": "warm", "name": "Warm", "color": "yellow"}]}}
        ],
        "views": [{
            "id": "pipeline", "name": "Pipeline", "type": "board", "order": "a0",
            "sort": [{"propertyId": "name", "direction": "ascending"}], "filter": null,
            "groupBy": "stage", "columns": [], "cardProperties": [], "collapsedGroups": []
        }]
    }
}"#).unwrap();

let rows: Vec<Doc> = serde_json::from_str(r#"[
    {"id": "r1", "kind": "page", "title": "Rowan Pike", "order": "a0",
     "parentDocId": "7d1c2f0e-4b8a-4e55-9d3c-2a6b1f0e9c11", "values": {"stage": "warm"}},
    {"id": "r2", "kind": "page", "title": "Imani Castell", "order": "a1",
     "parentDocId": "7d1c2f0e-4b8a-4e55-9d3c-2a6b1f0e9c11", "values": {"stage": "warm"}},
    {"id": "r3", "kind": "page", "title": "Theo Brandt", "order": "a2",
     "parentDocId": "7d1c2f0e-4b8a-4e55-9d3c-2a6b1f0e9c11", "values": {}}
]"#).unwrap();

let fields = database.database.as_ref().unwrap();
let pipeline = fields.view("pipeline").unwrap();
let now = 1_794_744_000_000; // milliseconds since the epoch
let shown = query(fields, pipeline, &rows, &Context::new(now));

let groups = shown.groups.unwrap();
assert_eq!(groups[0].key, GroupKey::Empty); // "No stage": Theo
assert_eq!(groups[2].key, GroupKey::Option("warm".into()));
let warm: Vec<&str> = groups[2].rows.iter().map(|&i| rows[i].title.as_str()).collect();
assert_eq!(warm, ["Imani Castell", "Rowan Pike"]);
```

`tests/fixtures` has a whole CRM in this JSON: `people.json`, `companies.json` and
`interactions.json` (each a database doc and its rows), `crm-page.json` (the page Interactions
sits inline in), and `events.json` (one of every event a view emits). All the names in them are
made up.

## Databases and rows are docs

Everything is a doc in the host's tree, the same shape for pages, databases and rows:

| Field | |
|---|---|
| `id` | A lowercase UUID, made by whoever creates the doc. |
| `kind` | `"page"` or `"database"` (other kinds are carried as they are). |
| `title` | The page's or database's name. For a row, its title property's value. |
| `order` | A fractional index among its siblings ([Order](#order)). For a row, its place in the database's manual order. |
| `folderId` | The folder a full-page doc is in, or null. |
| `parentDocId` | The page it's under, or null. A row's is its database. |
| `parentObjectId` | For an inline database, the block in its parent page that embeds it. |
| `cover`, `favorite`, `agents` | The host's; carried, never read. |
| `author`, `updatedBy` | Who created and last changed it: `{"kind": "owner"}` or `{"kind": "agent", "agentId": "career-planning"}`. |
| `created`, `updated` | Milliseconds since the Unix epoch. |
| `rev`, `seq` | The host's revision and sequence numbers; carried, never read. |
| `deleted` | A tombstone. Views leave deleted rows out. |
| `database` | Only on a database: `{properties: [Property], views: [View]}`. |
| `values` | Only on a row: `{propertyId: value}`. |

- **A full-page database** is a database doc in a folder or under a page.
- **An inline database** is a database doc whose `parentDocId` is the page that shows it and whose
  `parentObjectId` is the block that embeds it.
- **A row** is a page doc whose `parentDocId` is the database. Opening a row opens that page.
- A row's **created time** and **edited time** are its doc's `created` and `updated`; its creator
  and last editor are `author` and `updatedBy`. None of these are stored in `values`.

In Rust these are one type, `Doc`, with the fields above typed and a `database:
Option<DatabaseFields>` and `values: Option<Values>`. Every type keeps the JSON fields it doesn't
know in an `extra` map, and every enum keeps strings it doesn't know as `Other(String)`, so a doc
from a newer host or an agent reads and writes back unchanged. The round-trip tests check this
for every fixture.

## Properties and values

A property is `{id, name, type, config, order}`. Its `id` is a UUID, which rows' values are keyed
by, so renaming a property changes no row. The value's JSON depends on the type:

| Type | `config` | Value | Sorts by | Groups |
|---|---|---|---|---|
| `title` | | the row doc's `title` | text | |
| `text` | | `"..."` | text | |
| `number` | `format` (`plain`, `percent`, `currency` with `currency`, an ISO 4217 code), `precision` | `12.5` | number | |
| `select` | `options: [{id, name, color}]` | an option id | the options' order | a column per option |
| `multiSelect` | `options` | `[option id]` | the options' order | the row in each of its options |
| `date` | `includeTime`, `range` (what a new value gets) | `{start, end?, includeTime}` | `start` | relative, day, week, month, year |
| `checkbox` | | `true` / `false` (missing is false) | unchecked first | unchecked, checked |
| `url`, `email`, `phone` | | `"..."` | text | |
| `person` | `kinds: ["owner", "agent"]` | `[{kind: "owner"}, {kind: "agent", agentId}]` | the host's names | the row with each person |
| `relation` | `databaseId`, `twoWay`, `backPropertyId` | `[row doc id]` | the related rows' titles | |
| `createdTime` | | the doc's `created` | time | as dates |
| `editedTime` | | the doc's `updated` | time | as dates |

- **Option colours** are names (`default`, `gray`, `brown`, `orange`, `yellow`, `green`, `blue`,
  `purple`, `pink`, `red`); the host's theme says what each looks like.
- **Dates** are milliseconds since the epoch. Without `includeTime`, `start` and `end` are
  midnight UTC of the dates meant, and read as those dates in every time zone. With it they're
  instants, shown in the viewer's zone. A range's `end` is inclusive.
- **There's no contacts type.** People are rows in a People database, reached from other
  databases through a `relation` property. The `person` type is only ever the owner or one of
  their agents.
- **Text sorts** ignore case. Every sort puts empty values last, whichever way it runs, and is
  stable: rows that tie keep the database's manual order.
- **Missing and malformed values** are empty: a select whose option was deleted, a number stored
  as text, a shape the crate doesn't read. They're kept as they are, and never break a view.

## Views

A database has several saved views, each `{id, name, type, order, sort, filter, groupBy,
columns, cardProperties, collapsedGroups}`. They're tabs above the database, in `order`, and
every person (and agent) sees the same ones.

- **`sort`** is a list of `{propertyId, direction}` (`ascending` or `descending`), applied in
  turn. With no sort, rows are in the manual order (each row's `order`).
- **`filter`** is a tree of conditions ([Filters](#filters)).
- **`groupBy`** is a property id. Grouping by a date also reads **`groupDate`** (`relative`,
  `day`, `week`, `month`, `year`; relative when absent), an addition to the first proposal.
- **`columns`** is a table's `[{propertyId, width, hidden, order}]`. Properties without a column
  show after the others at the default width; `width` null means the default.
- **`cardProperties`** are the properties a board card or list line shows under the title.
- **`collapsedGroups`** are the groups folded shut, by key: `empty`, `option:<id>`, `checked`,
  `unchecked`, `owner`, `agent:<id>`, `relative:today`, `day:2026-11-15`, `week:2026-11-09`
  (the week's first day), `month:2026-11`, `year:2026`.

How each type will behave (the engine already works out what each shows; the views that draw
them come next):

- **Table.** A row per row and a column per property. Columns resize by dragging their edge and
  reorder by dragging their header; both are a `viewChanged`. The header's menu sorts, filters,
  hides and edits the property. Text, number, checkbox, select, multi-select, date, URL, email
  and phone cells edit in place; a person or relation cell opens a picker. The title cell has an
  Open button: opening a row is an `openRow` event, and the host shows the page. Grouped tables
  show each group under a header with its count, which folds. Dragging rows reorders them when
  the view has no sort (as in Notion, a sorted view can't be reordered by hand).
- **Board.** Columns of cards, one per group, grouped by a select (or a multi-select, checkbox,
  person or date). Dragging a card to another column changes its value, and its place in the
  manual order. A card shows its title and `cardProperties`. Each column has a + to add a row with
  that column's value.
- **List.** One line per row: the title, then `cardProperties` to the right. Grouping folds as in
  a table.
- **Gallery.** Cards in a grid. In the JSON from the start, drawn in a later version.

A new row made in a filtered or grouped view gets the values that keep it in that view (the
group's option, a checked box the filter wants), as Notion does.

## Filters

A filter is a tree: `{"and": [..]}` and `{"or": [..]}` over conditions `{propertyId, op,
value}`. For example, people to follow up with:

```json
{"and": [
  {"propertyId": "<Follow up>", "op": "isChecked"},
  {"or": [
    {"propertyId": "<Last contacted>", "op": "before", "value": "2026-10-15"},
    {"propertyId": "<Last contacted>", "op": "isEmpty"}
  ]}
]}
```

| Property type | Operators | `value` |
|---|---|---|
| title, text, url, email, phone | `is`, `isNot`, `contains`, `doesNotContain`, `startsWith` | a string; case is ignored |
| number | `is`, `isNot`, `greater`, `greaterOrEqual`, `less`, `lessOrEqual` | a number |
| select | `is`, `isNot` | an option id |
| multiSelect | `contains`, `doesNotContain` | an option id |
| person | `contains`, `doesNotContain` | a person, or `"owner"` or `"agent:<id>"` |
| relation | `contains`, `doesNotContain` | a row id |
| date, createdTime, editedTime | `is`, `isNot`, `before`, `after`, `onOrBefore`, `onOrAfter` | a day: `"2026-11-15"`, milliseconds, or a date value |
| date, createdTime, editedTime | `within` | `today`, `pastWeek`, `pastMonth`, `pastYear`, `nextWeek`, `nextMonth`, `nextYear` |
| checkbox | `isChecked`, `isNotChecked`, or `is` with `true` or `false` | |
| all but checkbox | `isEmpty`, `isNotEmpty` | |

- Dates compare whole days in the viewer's zone. A range matches when any of its days does.
  `within` spans include today.
- The negative operators (`isNot`, `doesNotContain`) match empty values, as Notion's do.
- A condition with no value yet (one being written) is left out. One whose property is gone, whose
  operator doesn't fit the type, or whose value can't be read is left out and reported as an
  `Issue`, which the view can show beside it. An `and` or `or` with nothing left matches every
  row.

## Edits and events

The crate never changes the host's data. Everything a person does in a view comes out as a
`TableEvent`:

- `{"type": "openRow", "rowId"}`: open the row's page. The crate never navigates.
- `{"type": "edits", "edits": [Edit]}`: changes to apply together, in one batch.

| Edit | Fields | The host |
|---|---|---|
| `rowCreated` | `rowId`, `databaseId`, `order`, `title`, `values` | makes a page doc under the database (filling in times, author, rev, seq) |
| `rowTitleChanged` | `rowId`, `title` | patches the row doc's `title` |
| `rowValueChanged` | `rowId`, `propertyId`, `value` | patches the row's `values` for that one property (null removes it) |
| `relationChanged` | `rowId`, `propertyId`, `databaseId`, `backPropertyId`, `added`, `removed` | writes the other side of a two-way relation ([Relations](#relations)) |
| `rowsMoved` | `rowIds`, `orders` | sets each row's `order` (`orders[i]` for `rowIds[i]`) |
| `rowsDeleted` | `rowIds` | tombstones the rows |
| `propertyCreated`, `propertyChanged` | `databaseId`, `property` | writes the whole property into the database's `database.properties` |
| `propertyDeleted` | `databaseId`, `propertyId` | removes it (rows' values for it may stay; they're ignored) |
| `viewCreated`, `viewChanged` | `databaseId`, `view` | writes the whole view |
| `viewDeleted` | `databaseId`, `viewId` | removes it |

How the host writes them to the server:

- **Row values and titles are patched, never put.** The server writes a doc's `values` only when
  the doc is created, and by `patch`. A `put` on an existing doc leaves its `values` alone.
  `patch` is the batch op `{op: "patch", type: "doc", id, title?, values?: {propertyId: value |
  null}}`, merged key by key, with the last write to each key winning. So:
  - `rowValueChanged {rowId, propertyId, value}` is `{op: "patch", type: "doc", id: rowId,
    values: {propertyId: value}}`, with null removing the value.
  - `rowTitleChanged {rowId, title}` is `{op: "patch", type: "doc", id: rowId, title}`. The title
    is the row doc's `title`, never a value in `values`, so a title edit is this event and not a
    `rowValueChanged`.
  - The edits of one batch for one row can go in one `patch`, with `title` and several `values`
    keys.
  - An agent and a person editing different cells of one row don't overwrite each other.
- **New rows** (`rowCreated`) are the only time a row's `values` are written whole.
- **Moving and deleting rows** set the docs' `order` and `deleted`, never their `values` (so a
  `put` of the doc is safe: it leaves `values` alone).
- **Properties and views are last-write-wins** inside the database doc's `database` field, and
  their edits carry the whole property or view.
- **One action, one batch.** Dropping a card in another board column is a `rowsMoved` and a
  `rowValueChanged`; changing a two-way relation is a `rowValueChanged` and a `relationChanged`.
  The host applies a batch together, and can undo it by applying the inverse.

For example, Theo dragged from Lead to Warm on the Pipeline board:

```json
{"type": "edits", "edits": [
  {"type": "rowsMoved", "rowIds": ["<Theo>"], "orders": ["a2V"]},
  {"type": "rowValueChanged", "rowId": "<Theo>", "propertyId": "<Stage>", "value": "<Warm>"}
]}
```

## Relations

A relation's value is a list of row doc ids in `config.databaseId` (which may be its own
database). A two-way relation is a pair of relation properties, one in each database, each naming
the other as `backPropertyId`: People's Company lists companies, and Companies' People lists
people back.

When one side changes, the batch has the change on that side and a `relationChanged` saying which
rows on the other side gain this row (`added`) and which lose it (`removed`). The host writes the
other side in the same batch; `engine::relation::back_edits` turns the `relationChanged` into
those rows' `rowValueChanged`s from the host's current values, and says which rows it couldn't
find or that are already full. `relation::set` makes the pair of edits, `relation::two_way_pair`
makes a new pair of properties, `relation::check_two_way` checks that a pair names each other, and
`RowIndex` resolves ids to rows (or lists the dangling ones).

The CRM in the fixtures has three two-way relations: People ↔ Companies (Company / People),
People ↔ Interactions (Interactions / People), and Companies ↔ Interactions (Interactions /
Company).

## Order

Rows, properties, views and columns are placed by `order` strings, fractional indexes that sort
by plain byte comparison. Moving something changes only its own key: `order::between(a, b)` makes
a key between two neighbours, `order::n_between` makes several for a multi-row drop, and
`order::spread(n)` makes fresh keys for renumbering a list.

The scheme is rocicorp's `fractional-indexing` (as Figma's before it): an integer part whose first
character gives its length (`a0` … `az`, `b00` …), then a base-62 fraction that never ends in `0`.
Keys are at most 256 characters. Appends step the integer part, so 10,000 appends make
four-character keys. The app's Swift and Go code is moving to the same scheme, and keys will
mix with the app's once it adopts it. Until then, and for any key that's malformed or too long (an
older scheme's, or an agent's), `between` returns an error rather than guessing, and the host
renumbers the list. A key that would grow past 256 characters is an error too (`tooLong`), and the host
renumbers.

`tests/fixtures/order-vectors.json` holds the cases every client tests against: `between`
(`{a, b, expected}` or `{a, b, error}`, with a or b null for the ends), `n_between` (`{a, b, n,
expected: [...]}`) and `spread` (`{n, expected: [...]}`). They cover a fresh list, appends,
prepends, adjacent keys, keys at and over the 256 cap, and invalid keys. The expected keys come
from `fractional-indexing` 3.4.0 itself. The one difference: a key with a character outside the
alphabet is invalid here, where that library accepts some.

## Limits

Agents write rows too, so `Limits` caps what a database and its rows may hold. The server checks
writes against the same numbers, the crate clamps what people type and paste
(`Limits::clamp_value`), and the engine ignores what's over (a filter too deep, a sort too many)
and reports it. Nothing over a limit panics. Strings count Unicode characters.

`Limits::check_doc` lists every problem with a doc, each a path from the doc's root and a message,
like `database.properties[3].config.options: 201 is more than 200` or `values.<phone>: 65
characters is more than 64` (`check_database`, `check_row` and `check_value` check the parts).
`tests/fixtures/limits` holds valid docs and invalid ones with the exact problems each must give,
one or more for every limit and check below. The crate's tests run them all, and the server runs
the same files. Its README gives the format and every message.

| Limit | Default |
|---|---|
| Properties per database | 200 |
| Columns and card properties per view, values per row | 200 (the properties limit) |
| Views per database | 50 |
| Sorts per view | 10 |
| Options per select or multi-select | 200 |
| Values per multi-select | 100 |
| Values per relation (a two-way back side too) | 2,000 |
| People per person value | 50 |
| Collapsed groups per view | 500 |
| Filter depth (a lone condition is 1) | 4 |
| Filter nodes (groups and conditions) | 100 |
| Title (a row's or a database's) | 2,000 characters |
| Text | 10,000 characters |
| URL | 2,048 characters |
| Email | 320 characters |
| Phone | 64 characters |
| Option name | 100 characters |
| Property or view name | 200 characters |
| Any id | 128 characters |
| An `order` key | 256 characters |

The checks also catch ids used twice, a database with no title property or two, a value of the
wrong shape for its type, a select or multi-select value naming an option the property hasn't, a
person of a kind the property doesn't allow, a date range ending before it starts, and a value
stored for a created or edited time. A filter depth of 4 is three levels of nested groups and
their conditions, as Notion allows.

## The host's part

The host is the app around the views. It:

- **Stores everything.** It hands the views the database doc and its row docs, applies the edits
  they emit (to its store and its server), and hands back the changed docs. The views redraw only
  when it does.
- **Opens rows.** On `openRow` it shows the row's page, with gpui-blocks in the docs app. The crate
  never navigates and knows nothing of page content.
- **Knows people and agents.** Through a host trait it gives the names of the owner and agents,
  the titles of related rows, the list of people and agents for the person picker, and rows of a
  related database for the relation picker. The engine's `Lookup` trait is the part sorting and
  grouping need.
- **Gives the theme.** Colours (text, lines, fills, the ten option colours, the accent), fonts,
  sizes and row heights, scaled by the window's rem, as gpui-chatwidgets' `CardTheme` does.
- **Writes both sides of relations** in one batch, with `relation::back_edits`.
- **Checks agents' writes** against `Limits`.

## Performance

The target: a 10,000-row database scrolls, filters, sorts and regroups with no dropped frame, and
nothing draws while nothing changes.

- **The engine** builds each row's keys once and sorts indexes, never rows. Measured with `cargo
  test --release --test performance -- --nocapture` on an Apple M4 Pro, for 10,000 rows of every
  property type:

  | | median |
  |---|---|
  | The manual order alone | 0.06 ms |
  | Sort by title | 1.5 ms |
  | Sort by a select, then a number | 1.6 ms |
  | A filter of three conditions, two sorts, grouped by a select | 1.4 ms |
  | Twelve conditions, one per type | 1.7 ms |
  | Group by multi-select / person / date | 0.7 / 1.0 / 0.5 ms |
  | Read 10,000 rows from JSON (5.4 MB) | 17 ms |
  | Check 10,000 rows against the limits | 2.3 ms |
  | 10,000 fresh order keys | 0.4 ms |

  The tests fail past bounds well above these (25 ms for any of a view's work in a release build,
  forty times that in a debug one), loose enough for a shared CI runner and tight enough to catch
  a step that goes quadratic.
- **The views** will draw only the rows on screen (GPUI's `uniform_list` for a table or list, a
  virtualized column per board group), work out a view again only when the docs or the view
  change, and keep no timers. No text will overflow its cell at any width: cells clip or end in an
  ellipsis, and a headless Core Text test, as gpui-chatwidgets has, will check every cell type at
  narrow and wide widths.

## iOS and touch

The views are for the Mac and for iPhone and iPad, through GPUI's iOS backend (`gpui-pre-ios`
0.3.12). The crate builds for `aarch64-apple-ios-sim`, and CI checks it.

- Every control is at least 44 points on touch: cell targets, header menus, the Open button,
  group headers and column resize handles (whose visible line stays thin).
- Nothing is hover-only. What a pointer finds by hovering (the Open button, a column's menu, a
  card's handle) is a tap away on touch.
- A tap selects a cell and a second tap edits it; pickers open as popovers on iPad and sheets on
  iPhone. A long press picks up a card or row to drag. Tables scroll both ways with momentum, and
  the title column stays put.

## Not in v1

- **Formulas.** They need an expression language, an evaluator, dependency tracking between
  properties, and a server that computes the same answers, which is as much work as the rest of
  the crate. Agents can write computed values into ordinary properties in the meantime.
- **Rollups.** A rollup aggregates a property across related rows, so it needs the related
  databases loaded together and recomputes whenever any related row changes. That's worth doing
  once relations have been used for a while.
- **Templates.** A template is a page with content, which belongs to the docs app and its editor;
  the crate would only pass a template id through `rowCreated`. Prefilled values for a new row in a
  view are in v1.
- **Also later:** the gallery view, created-by and edited-by properties (the docs carry `author`
  and `updatedBy` already), files, a status type, unique ids, sums and counts under a column, and
  linked views of a database on another page.

## Tests

```sh
cargo test -p gpui-tables                              # everything
cargo test -p gpui-tables --no-default-features        # the same, with no GPUI in the build
cargo test -p gpui-tables --release --test performance -- --nocapture
```

- `tests/model.rs`: every fixture reads into the typed model and writes back as the same JSON;
  values read as their types; the fixtures are within the limits and their relations agree on
  both sides; unknown fields and kinds survive; over-limit and malformed input is reported or
  clamped.
- `tests/engine.rs`: the fixtures' saved views work out as a person would expect; every operator
  on every type; sorts, groups and their keys; relations and back edits; what the engine leaves
  out and reports; and hostile values (every shape in every property, every operator with every
  value, the extremes of time) that must never panic.
- `tests/order.rs`: every case in `tests/fixtures/order-vectors.json` ([Order](#order)).
- `tests/limits.rs`: every case in `tests/fixtures/limits`, each with exactly its problems
  ([Limits](#limits)).
- `tests/performance.rs`: 10,000 rows, timed, as above.
- Unit tests for fractional indexes and calendar arithmetic.

## License

Apache-2.0. See [LICENSE](../LICENSE) and [NOTICE](../NOTICE).
