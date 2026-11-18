# gpui-cli

`gpui add`: copies [gpui-core](../gpui-core)'s components into your crate, the way
shadcn/ui's CLI does for React, for you to own and change.

```sh
cargo install --locked --git https://github.com/trevin-lee/gpui-essentials gpui-cli
gpui add select dialog      # into src/ui/, with what they need
gpui add --all
gpui add select --diff      # what a newer gpui-core changed in your copy
gpui list
```

It's a git install only: the crates.io name `gpui-cli` belongs to an unrelated project
(AprilNEA/gpui-cli 0.0.1), so a published version would need another name.

Then add `mod ui;` to your crate root and call `ui::init(cx)` at startup. gpui-core's README
("Copy a component into your crate") covers the options, what changes in a copy, and how
updates work. `gpui --help` lists the options.

`ci/check-copies.sh` is the CI check: it copies every component into a new crate, builds it,
and runs a copy next to gpui-core's original in one app.
