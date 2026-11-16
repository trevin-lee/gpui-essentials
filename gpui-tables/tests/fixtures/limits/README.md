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
characters, not bytes.

| Message | When |
|---|---|
| `N is more than L` | Too many: properties, views, sorts, options, columns, card properties, collapsed groups, filter nodes, values in a list, or values in a row |
| `N characters is more than L` | A string too long: a title, a text, URL, email or phone value, a name, an id, an `order` key |
| `N levels is more than L` | A filter too deep (a lone condition is 1 level) |
| `<id> is used twice` | Two properties, views, options or columns share an id, or a list value repeats one |
| `no title property` | A database without a title property |
| `more than one title property` | A database with two or more |
| `not a <type> value` | A value of the wrong shape for its property's type (`<type>` as in the JSON, e.g. `multiSelect`) |
| `no option <id>` | A select or multi-select value naming an option the property hasn't |
| `a kind of person this property doesn't allow` | A person not in the property's `kinds`, or of a kind the crate doesn't know |
| `ends before it starts` | A date range whose `end` is before its `start` |
| `derived, never stored` | A value stored for a created time or edited time |

Values for properties the database hasn't, and null values, aren't checked (they count toward
the 200 values a row may have).

The cases were written by a one-off script, and are kept as data, not regenerated. When a limit or
a check changes, edit the cases; the crate's tests say which no longer match.
