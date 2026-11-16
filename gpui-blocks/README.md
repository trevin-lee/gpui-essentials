# gpui-blocks

A Notion-style block editor for [GPUI](https://www.gpui.rs) apps built with
[GPUI Kit](https://github.com/longbridge/gpui-kit) 0.9. Documents are blocks you can type into,
turn into other blocks with `/`, and drag around, and they're kept as Markdown.

It uses GPUI Kit's types throughout (GPUI Kit builds on `gpui-pre`, a snapshot of Zed's GPUI), so
it works in GPUI Kit apps but not with Zed's own `gpui` crate.

![A document in gpui-blocks: headings, a to-do list, a numbered list, callouts and toggles](docs/doc.png)

## What it does

- **Blocks:** paragraphs, headings (three in the slash menu, all six with `#`), nested bullet,
  numbered and to-do lists, quotes, callouts, toggles, code with syntax highlighting, dividers,
  tables you can edit, pictures, and a table of contents that follows the headings.
- **Slash menu:** type `/` at the start of a line or after a space. Narrow the list by typing,
  pick with ↑ ↓ and Enter, and close it with Esc.
- **Side menu:** hover a block for its handles. "+" adds a line below with the menu open, and the
  grip drags the block.
- **Motion:** while you drag, the blocks you pass slide aside to open a gap, and the block
  springs into it when you let go. Moving a top-level block with ⌘⇧↑ or ↓ (Ctrl on Linux and
  Windows), or undoing a move, slides the blocks to their new places. List items move within
  their list, without motion.
- **Markdown shortcuts as you type:** `# `, `- `, `1. `, `[ ] `, `> `, `**bold**`, `*italic*`,
  `~~strike~~`, `` `code` `` and `[text](url)`, and three backticks, `---` or `| a | b |`
  followed by Enter.
- **Editing:** undo and redo, selection across blocks, IME, and copy and paste as Markdown.
- **Touch:** on an iPhone or iPad, a tap puts the caret there and brings up the keyboard, a long
  press selects a word, handles at the selection's ends move them, and a menu over the
  selection cuts, copies, pastes and selects all.
- **Widgets:** blocks your app draws itself, such as a chart, kept in the file as fenced code.
- **Links to other documents:** mentions inside text and sub-page blocks, drawn with the title
  and icon your app gives them.

| The slash menu | The side menu | Dragging a block |
|---|---|---|
| ![The slash menu open under a "/"](docs/slash.png) | !["+" and a grip beside a to-do list](docs/hover.png) | ![A heading carried down past a to-do list, which slides up to make room](docs/drag.png) |

## Use it

It needs Rust 1.100 or newer, as GPUI Kit 0.9 does.

```toml
[dependencies]
gpui-kit = "0.9.1"
gpui-blocks = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
```

Pin a commit: until it's on crates.io, any commit can change the API.

```rust,no_run
use gpui_kit::*;

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        gpui_blocks::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            let editor = cx.new(|cx| gpui_blocks::Editor::new("# Hello\n\nType `/` for blocks.", window, cx));
            editor.update(cx, |editor, cx| editor.focus(window, cx));
            editor
        })
        .unwrap();
    });
}
```

Try the example. It rewrites a file you open in the editor's own Markdown (see below), so try it
on a copy:

```sh
cargo run --release --example editor               # a sample document, not saved
cargo run --release --example editor -- notes.md   # a file of yours, saved as you type
```

## Putting it in an app

- **Layout:** `Editor` fills the view it's in and scrolls itself. The text sits in a 720 px
  column, with room on its left for the side menu.
- **Look:** it draws with GPUI Kit's `Theme` and paints no background, so put it in a view that
  does (`.bg(cx.theme().background)`). The pictures here use the example's dark theme
  (`apply_theme` in `examples/editor.rs`).
- **The document:** `editor.markdown(cx)` returns it and `editor.set_markdown(..)` replaces it.
  YAML front matter at the top is kept aside and written back unchanged.
  `editor.set_readonly(true, cx)` blocks edits and hides the side menu.
- **Events** (`EditorEvent`): `Changed` after every edit. `EditLinkRequested { url }` on ⌘K: show
  a field for the link, then call `set_link` on `editor.state()`. `LinkClicked { url }` on
  ⌘-click: the editor never opens links itself. The events for links to other documents are
  under [Links to other documents](#links-to-other-documents).
- **Pictures:** `Options::images(..)` loads them: it turns a picture's address into the image
  to draw, from your app's files, cache or storage, under your rules about the network. Without
  it a picture is drawn as its description, and the editor never reads a file or fetches an
  address a document names. The example loads image files from the open document's folder.
- **Keys:** the view's actions are in `gpui_blocks::actions`, and its key context is
  `BlocksEditor`. Bind them to other keys after `gpui_blocks::init`.
- **Everything else:** `editor.state()` is the underlying gpui-kit-markdown-editor
  `BlockEditorState` (see below). Its API follows the fork and can change with it.

## Widgets

A widget is a fenced block whose info string is the widget's name:

````markdown
```chart
kind: bar
values: 3, 5, 2
```
````

Your app draws it, and any other Markdown tool shows it as a code block. In the editor it's one
unit, like a picture: it's selected, moved and deleted whole. It's written back with the same
name, info string and body, inside quotes, callouts and lists too.

```rust,no_run
use gpui_blocks::slash::{SlashAction, SlashItem};
use gpui_blocks::widget::{Widget, fenced};
use gpui_blocks::{Editor, Options};
use gpui_kit::*;

struct Chart;

impl Widget for Chart {
    fn name(&self) -> &str {
        "chart"
    }

    fn render(&self, body: &str, _window: &mut Window, _cx: &mut App) -> AnyElement {
        // Read `body` and draw the chart.
        div().child(body.to_string()).into_any_element()
    }
}

fn editor(markdown: &str, window: &mut Window, cx: &mut App) -> Entity<Editor> {
    let options = Options::default().widget(Chart).slash_item(
        SlashItem::new("Chart", SlashAction::Markdown(fenced("chart", "kind: bar").into()))
            .hint("A bar chart")
            .aliases(["graph", "plot"]),
    );
    cx.new(|cx| Editor::with_options(markdown, options, window, cx))
}
```

A widget draws from its body alone: it gets no block id and can't write a new body back, so two
widgets with the same body look the same to it. The example's `answer` card
(`examples/editor.rs`) is a complete one.

## Links to other documents

An app that keeps many documents links them by id. Both kinds of link are ordinary Markdown
links whose destination is `doc:` and the id:

```markdown
The plan is in [Roadmap](doc:4f2a), and last week has a page of its own:

[Last week](doc:9c1e "page")
```

- A **mention** is a `doc:` link inside text. It's drawn as the document's icon and title, and
  edited as one unit.
- A **sub-page** is a line holding only a `doc:` link titled `page`. It's a block, moved and
  deleted whole.

The editor asks your app for each document's current title and icon, so a link never goes stale
when its document is renamed. The text in the file is the title when the link was written, shown
only while your app has no title for the id. An id is 1 to 128 of `A-Z a-z 0-9 - _ . ~`; any
other `doc:` link stays a plain link.

```rust,no_run
use gpui_blocks::docs::{DocInfo, Docs};
use gpui_blocks::{Editor, EditorEvent, Options};
use gpui_kit::*;

struct MyDocs;

impl Docs for MyDocs {
    fn info(&self, id: &str, _cx: &App) -> Option<DocInfo> {
        // From the app's own cache. None for a document that's gone.
        (id == "4f2a").then(|| DocInfo::new("Roadmap").icon("🗺"))
    }
}

fn editor(markdown: &str, window: &mut Window, cx: &mut App) -> Entity<Editor> {
    let editor =
        cx.new(|cx| Editor::with_options(markdown, Options::default().docs(MyDocs), window, cx));
    cx.subscribe(&editor, |editor, event, cx| match event {
        EditorEvent::DocClicked { id } => { /* open the document */ }
        EditorEvent::NewPageRequested => {
            // Make the document, then put it in this one.
            editor.update(cx, |e, cx| e.insert_page("new-id", "Untitled", cx));
        }
        EditorEvent::DocLinkRequested => { /* show a picker, then call insert_mention */ }
        _ => {}
    })
    .detach();
    editor
}
```

`Options::docs` also adds Page and Link to page to the slash menu, which send
`NewPageRequested` and `DocLinkRequested`. A click on a link sends `DocClicked`, and the editor
never opens anything itself. After your documents change, call `editor.refresh_docs(cx)` to
draw the links again.

## As JSON

`editor.document(cx)` gives the document as a tree of blocks, each with an id that stays the same
while the block is edited and moved, and `editor.set_document(..)` loads one back with its ids.
Store this when something has to point at a block across saves: links between documents,
comments, search. Text inside blocks stays inline Markdown.

```json
{ "format": "gpui-blocks", "version": 1, "blocks": [
  { "id": "Zr81mXbT0qvN", "type": "paragraph", "text": "The plan is in [Roadmap](doc:4f2a)." },
  { "id": "c7HhQ2wLx5Ke", "type": "page", "doc": "9c1e", "title": "Last week" }
] }
```

`gpui_blocks::json::Document` converts to and from Markdown without an editor, and lists the
documents one links to (`links()`, `pages()`). [docs/json.md](docs/json.md) describes every block
type, for reading the JSON without this crate.

## Importing notes

`gpui_blocks::import::note(path, text, &targets)` reads one Markdown file exported from Notion
or kept in an Obsidian vault, and gives its title and its document as JSON. Your app gives every
note a document id first, then answers `Targets` with them, so each link to another note
becomes a mention of that note's document:

- Notion's page ids are taken off file names, and its URL-encoded links between pages, its
  `<aside>` callouts and the title heading every page opens with are handled.
- Obsidian's `[[links]]`, `![[embeds]]` (a note embedded on its own line becomes a sub-page),
  callouts of any kind (a foldable one becomes a toggle) and `==highlights==` are handled.
- Pictures and other files load from where your app put them, or are left as their
  description. A link out of the import, or to a note that wasn't imported, is left as its
  text. Code is left alone.

## The Markdown it writes

CommonMark, with GitHub's tables and task lists:

- **Callouts** are GitHub's alerts (`> [!NOTE]`, `> [!WARNING]` and the rest, with the marker
  alone on its line). GitHub and Obsidian draw them too.
- **Toggles** are HTML's `<details>` with a `<summary>`. The body is written at the toggle's own
  level between blank lines, so it stays Markdown, and an open toggle says `open`.
- **Widgets** are fenced blocks, as above.
- **Links to other documents** are links to `doc:` and an id, as above.

Saving writes the editor's own form of Markdown, so a file it didn't write can change in how
it's written, though not in what it says:

- one way to write each thing: `*italic*`, `**bold**`, `-` bullets, `1.` numbers, `#`
  headings, backtick fences, `---` rules, `[x]` for done tasks, and tables padded with
  `| --- |` rows;
- reference links become inline links, and entities such as `&copy;` become their characters;
- one blank line between blocks, line breaks within a paragraph become spaces, hard breaks
  become `<br>`, and line endings become LF;
- a `$` is written `\$`, because the parser reads `$…$` as math.

A document the editor wrote reads back to the same text.

## Performance

Measured with `cargo run --release --example editor -- --bench` on an Apple Silicon Mac, in a
1280×820 window at 2x on a 120 Hz display:

| | |
|---|---|
| Keystroke to painted frame | not measured yet since the bench was corrected (it stopped its clock before the frame was drawn) |
| Opening a 5,000-line document (152 KB) | not measured yet, for the same reason |
| Scrolling it end to end | every frame at 120 a second, about 2 ms of work each (median) |
| Memory (footprint, as Activity Monitor counts it) | 85 MB with the sample open; 162 MB with the 5,000-line document |
| Idle, window in the background | nothing drawn, no CPU |

"Painted" is when the frame has been drawn, before the display shows it at its next refresh.
Nothing draws while nothing changes, apart from the caret's blink twice a second while the editor
has focus. Blocks being moved draw every frame until they come to rest: most of the way in about
150 ms, and at rest after 160 to 390 ms, depending on how far they move.

## How it's built

The document model, input, selection, undo and text drawing come from
[gpui-kit-markdown-editor](https://github.com/wabzqem/gpui-kit-markdown-editor), which draws
through GPUI Kit's editor host API. They come through
[a fork](https://github.com/trevin-lee/gpui-kit-markdown-editor/tree/oca) that adds callouts,
toggles, block motion, and the hooks this crate needs to draw its menus. This crate adds the slash
menu, the side menu, the widget API and the example.

## Status

Early, and used for one app so far. It's tested on macOS, and CI builds it for iOS, where its
touch input is tested headlessly but not yet on a device. GPUI also runs on Linux and Windows,
but this crate hasn't been tried there.

It isn't on crates.io yet. It depends on the fork by git commit, and crates.io won't accept a git
dependency. The fork's changes haven't been offered upstream yet.

Not done yet:
- Dragging blocks nested inside lists, quotes and toggles.
- A menu on the drag handle (turn into, duplicate, delete), and Esc to select a block.
- Pasting or dropping pictures.
- A link field for ⌘K. The editor sends `EditLinkRequested`, and the app shows the field.
- Typing `@` to search for a document to mention. For now that's the slash menu's Link to page,
  and the app's own picker.
- Table grips in centred and right-aligned columns land slightly off.

## License

Apache-2.0, as gpui-kit-markdown-editor and GPUI Kit are. See [LICENSE](../LICENSE) and
[NOTICE](../NOTICE). Notion is a trademark of Notion Labs, Inc., and this project isn't affiliated
with it.
