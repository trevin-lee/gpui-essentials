# gpui-essentials

Components for [GPUI](https://www.gpui.rs) apps, one crate per folder, built and tested
together so they link one GPUI.

| Crate | What it is |
|---|---|
| [gpui-blocks](gpui-blocks) | A Notion-like block editor: Markdown documents with a slash menu, drag handles, callouts, toggles and widget blocks |
| [gpui-calendar](gpui-calendar) | Day, week and month calendar views with a time grid, drag to create, move and resize, and time zones |
| [gpui-chatwidgets](gpui-chatwidgets) | Draws AI agents' [OpenUI](https://openui.com) cards (cards, tables, charts, forms, buttons) natively, with a parser that builds without GPUI |
| [gpui-core](gpui-core) | The components the others build on (theme, root, buttons, inputs, markdown text), drawn in the host's theme; replacing GPUI Kit, in progress |
| [gpui-flow](gpui-flow) | A node-and-edge editor in the manner of React Flow |
| [gpui-tables](gpui-tables) | Notion-style databases: typed properties, saved table, board and list views, filters, sorts and groups, with a model and engine that build without GPUI |

Each folder has its own README with what the crate does, how to use it, its example window and
its tests.

## Use them

They need Rust 1.100 or newer and draw through [GPUI Kit](https://github.com/longbridge/gpui-kit),
pinned exactly to `=0.9.1` (which pins Zed's GPUI as `gpui-pre` 0.3.12). Your app must use the
same versions, or two GPUIs would be linked. Take any of them from this repository by name,
pinned to one commit, the same commit for all of them:

```toml
[dependencies]
gpui-kit = "=0.9.1"
gpui-calendar = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
gpui-chatwidgets = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
```

None of them is on crates.io yet, and any commit can change an API, so pin one.

## Working here

```sh
cargo test -p gpui-calendar                     # one crate
cargo test --workspace                          # all of them
cargo run -p gpui-flow --release --example flow # an example window
```

`rust-toolchain.toml` pins Rust 1.100.0, and rustup installs it on the first build. The examples
and tests need macOS: GPUI draws through Metal. Each crate has its own CI workflow in
`.github/workflows`, run on GitHub's macOS runners when its folder or the shared files change;
gpui-chatwidgets' parser-only build and gpui-tables' engine-only build are also tested on Linux.

## History

gpui-blocks and gpui-calendar began as repositories of their own, and gpui-chatwidgets as a
local one; their histories are kept here, each in its folder. gpui-flow came in as one commit.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE), which carries every crate's
third-party notices: GPUI Kit, the gpui-kit-markdown-editor fork gpui-blocks uses, OpenUI (MIT)
for gpui-chatwidgets, and the Inter font in the examples.
