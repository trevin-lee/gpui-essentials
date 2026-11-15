# gpui-blocks

A Notion-like block editor for [GPUI](https://www.gpui.rs), the GPU-drawn UI framework from Zed.
Documents are blocks you can type into, turn into other blocks with `/`, and drag around, and
they're kept as plain Markdown.

![A document in gpui-blocks: headings, a to-do list, a numbered list, callouts and toggles](docs/doc.png)

## What it does

- **Blocks:** paragraphs, three heading levels, nested bullet, numbered and to-do lists, quotes,
  callouts, toggles, code with syntax highlighting, dividers, tables you can edit, and pictures.
- **Slash menu:** type `/` at the start of a line or after a space. Narrow the list by typing,
  pick with ↑ ↓ and Enter, and close it with Esc.
- **Side menu:** hover a block for its handles. "+" adds a line below with the menu open, and the
  grip drags the block.
- **Motion:** while you drag, the blocks you pass slide aside to open a gap, and the block
  springs into it when you let go. Moving a block with ⌘⇧↑ or ↓, or undoing a move, slides the
  blocks to their new places.
- **Markdown shortcuts as you type:** `# `, `- `, `1. `, `[ ] `, `> `, three backticks, `---`,
  `**bold**`, `*italic*`, `` `code` ``, `[text](url)` and `| a | b |`.
- **Editing:** undo and redo, selection across blocks, IME, and copy and paste as Markdown.
- **Widgets:** blocks your app draws itself, such as a chart or an assistant's answer, kept in
  the file as fenced code.

| The slash menu | Dragging a block |
|---|---|
| ![The slash menu open under a "/"](docs/slash.png) | ![A heading carried past a to-do list, which slides up to make room](docs/drag.png) |

## Use it

```toml
[dependencies]
gpui-kit = "0.9.1"
gpui-blocks = { git = "https://github.com/trevin-lee/gpui-blocks" }
```

```rust
use gpui_kit::*;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        gpui_blocks::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| gpui_blocks::Editor::new("# Hello\n\nType `/` for blocks.", window, cx))
        })
        .unwrap();
    });
}
```

`Editor` emits `EditorEvent::Changed` after every edit, and `editor.markdown(cx)` returns the
document. The example saves a file 400 ms after typing stops.

Try the example:

```sh
cargo run --release --example editor               # a sample document, not saved
cargo run --release --example editor -- notes.md   # a file of yours, saved as you type
```

## Widgets

A widget is a fenced block whose info string is the widget's name:

````markdown
```chart
kind: bar
values: 3, 5, 2
```
````

Your app draws it, and any other Markdown tool shows it as a code block. In the editor it's one
unit, like a picture: it's selected, moved and deleted whole, and written back exactly as it was
read.

```rust
use gpui_blocks::slash::{SlashAction, SlashItem};
use gpui_blocks::widget::{Widget, fenced};
use gpui_blocks::{Editor, Options};

struct Chart;

impl Widget for Chart {
    fn name(&self) -> &str {
        "chart"
    }

    fn render(&self, body: &str, _: &mut Window, cx: &mut App) -> AnyElement {
        // Read `body` and draw the chart.
        div().child(body.to_string()).into_any_element()
    }
}

let options = Options::default().widget(Chart).slash_item(
    SlashItem::new("Chart", SlashAction::Markdown(fenced("chart", "kind: bar").into()))
        .hint("A bar chart")
        .aliases(["graph", "plot"]),
);
let editor = cx.new(|cx| Editor::with_options(markdown, options, window, cx));
```

The example's `answer` card (`examples/editor.rs`) is a complete one.

## The Markdown it writes

CommonMark, with GitHub's tables and task lists:

- **Callouts** are GitHub's alerts (`> [!NOTE]`, `> [!WARNING]` and the rest, with the marker
  alone on its line). GitHub and Obsidian draw them too.
- **Toggles** are HTML's `<details>` with a `<summary>`. The body is written at the toggle's own
  level between blank lines, so it stays Markdown, and an open toggle says `open`.
- **Widgets** are fenced blocks, as above.

Reading a document and writing it back gives the same text, with two exceptions. A `$` is written
`\$`, because the parser reads `$…$` as math. A paragraph's line breaks become spaces.

## Performance

Measured with `cargo run --release --example editor -- --bench` on an Apple Silicon Mac, in a
1280×820 window at 2x on a 120 Hz display:

| | |
|---|---|
| Keystroke to painted frame | median 3.7–4.8 ms; worst 8.3 ms in a 5,000-line document |
| Opening a 5,000-line document (152 KB) | 45 ms to its first frame |
| Scrolling it end to end | every frame at 120 a second, about 2 ms each |
| Memory | 85 MB with the sample open; 162 MB with the 5,000-line document |
| Idle | nothing drawn, no CPU |

Nothing draws while nothing changes. Blocks being moved draw every frame only until they've
settled, about 150 ms.

## How it's built

The document model, input, selection, undo and text drawing come from
[gpui-kit-markdown-editor](https://github.com/wabzqem/gpui-kit-markdown-editor), which draws
through [GPUI Kit](https://github.com/longbridge/gpui-kit)'s editor host API. They come through
[a fork](https://github.com/trevin-lee/gpui-kit-markdown-editor) that adds callouts,
toggles, block motion, and the hooks this crate needs to draw its menus. This crate adds the slash
menu, the side menu, the widget API and the example.

## Status

Early, and used for one app so far. It's tested on macOS. GPUI also runs on Linux and Windows,
but this crate hasn't been tried there.

It isn't on crates.io yet. It depends on the fork by git commit, and crates.io won't accept a git
dependency, so publishing waits until the fork's changes are released.

Not done yet:
- Dragging blocks nested inside lists, quotes and toggles.
- A menu on the drag handle (turn into, duplicate, delete), and Esc to select a block.
- Pasting or dropping pictures.
- A link editor for ⌘K. The editor emits the request, but the host has to show the field.
- Table grips in centred and right-aligned columns land slightly off.

## License

Apache-2.0, as gpui-kit-markdown-editor and GPUI Kit are. See [LICENSE-APACHE](LICENSE-APACHE).
