# Limit vectors

Cases for checking docs against gpui-tables' `Limits` (the defaults, as the crate's README lists
them). The crate's `tests/limits.rs` runs every case through `Limits::check_doc`, and the server
runs the same files against its own checks. Both must give exactly the listed problems.

- `valid.json`: docs with no problems, including a database and a row at every limit, none
  over.
- `invalid.json`: docs that each break one limit or check (a few break two), with the problems
  they must give.

## The format

Each file is a JSON array of cases, one per line:

```json
{"name": "201 options", "doc": {...}, "problems": ["database.properties[3].config.options: 201 is more than 200"]}
{"name": "a phone of 65 characters", "database": {...}, "doc": {...}, "problems": ["values.phone: 65 characters is more than 64"]}
```

- `name` says what the case is.
- `doc` is the doc to check: a database doc (kind `"database"`, checked with its `database`
  field) or a row (kind `"page"`).
- `database`, present for a row, is the database doc the row is in. Its properties say what
  the row's values should be.
- `problems` lists every problem the doc has, in any order (compare them sorted). Each is a
  path, `: `, then a message.

## Paths

A path starts at the doc's root, in the docs' own field names: `id`, `order`, `title`,
`database.properties[3].config.options[0].name`, `database.views[0].filter`, `values.<property
id>`, `values.<property id>[2]` (an item of a list value), `values.<property id>[1].agentId`.
Indexes count from 0.

## Messages

Each message is one of these, with N the number found and L the limit. Strings count Unicode
characters, not bytes, except a doc's `icon`, which counts extended grapheme clusters (UAX #29)
and UTF-8 bytes.

| Message | When |
|---|---|
| `N is more than L` | Too many: properties, views, sorts, options, columns, card properties, collapsed groups, calculations (a view's, at most one per property), filter nodes, values in a list, or values in a row |
| `N characters is more than L` | A string too long: a title, a text, URL, email or phone value, a name, an id (a view's `calculations` keys too, at the path `database.views[i].calculations`), an `order` key |
| `N levels is more than L` | A filter too deep (a lone condition is 1 level) |
| `<id> is used twice` | Two properties, views, options or columns share an id, or a list value repeats one |
| `no title property` | A database without a title property |
| `more than one title property` | A database with two or more |
| `not a <type> value` | A value of the wrong shape for its property's type (`<type>` as in the JSON, e.g. `multiSelect`) |
| `no option <id>` | A select or multi-select value naming an option the property hasn't |
| `a kind of person this property doesn't allow` | A person not in the property's `kinds`, or of a kind the crate doesn't know |
| `ends before it starts` | A date range whose `end` is before its `start` |
| `not none or cover (view <id>)` | A view's `cardPreview` that's a string other than `none` or `cover` (absent or null is none), at the path `database.views[i].cardPreview` |
| `not small, medium or large (view <id>)` | A view's `cardSize` that's a string other than `small`, `medium` or `large` (absent or null is medium), at the path `database.views[i].cardSize` |
| `derived, never stored` | A value stored for a created time, edited time, created-by or edited-by (any value but null) |
| `N graphemes is more than 1` | An `icon` (not starting with `asset:`) of more than one grapheme cluster |
| `N bytes is more than 16` | An `icon` (not starting with `asset:`) over 16 bytes in UTF-8 |
| `an empty string` | An `icon` of `""` (no icon is null) |
| `not asset: and 64 lowercase hex characters` | An `icon` starting with `asset:` not followed by exactly 64 characters of `0-9a-f` |
| `not a string or null` | An `icon` that's a number, a bool, an array or an object |

An icon's path is `icon`, on a doc of any kind. A text icon over 16 bytes gives both its
graphemes and its bytes (`"seventeen letters"`: `icon: 17 graphemes is more than 1` and `icon:
17 bytes is more than 16`); an `asset:` value is checked only as an asset, never for its
graphemes or bytes.

Values for properties the database hasn't, and null values, aren't checked (they count toward
the 200 values a row may have).

A select and a multi-select each take both shapes, since a property changed from one to the other
keeps its rows' values: an option id (`"lead"`) or a list of option ids (`["lead", "won"]`, which a
select reads as its first). The checks are the same for both types: a lone id is an id (at most
128 characters) naming one of the property's options (path `values.<id>`); a list holds at most
100 ids (the multi-select limit, a select's list too), none twice, each an id naming an option
(path `values.<id>[2]`). An empty list is no option. Any other shape is `not a select value` or
`not a multiSelect value`.

The cases were written by a one-off script, and are kept as data, not regenerated. When a limit or
a check changes, edit the cases; the crate's tests say which no longer match.
