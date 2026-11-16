//! A block editor for [GPUI](https://www.gpui.rs) apps built with GPUI Kit.
//!
//! A document is blocks (paragraphs, headings, nested bullet, numbered and to-do lists, quotes,
//! callouts, toggles, code with its language, dividers, tables, pictures) and widgets, blocks an
//! app draws itself. It's edited the way block editors such as Notion work, and kept as Markdown:
//!
//! - **Slash menu** ([`slash`]): `/` lists the blocks the line can become, narrowed by what's
//!   typed after it.
//! - **Side menu**: beside the block under the pointer, "+" to add a line below and a handle to
//!   drag the block. The blocks it passes slide aside, and it springs into place when let go.
//! - **Widgets** ([`widget`]): fenced blocks an app draws, such as a chart, kept as fenced code
//!   so any Markdown tool can still read the document.
//! - **Links to other documents** ([`docs`]): mentions inside text and sub-page blocks, drawn
//!   with the title and icon the app gives them.
//! - **JSON** ([`json`]): the document as a tree of blocks with ids that last, for storing
//!   and indexing.
//! - Markdown shortcuts as you type (`# `, `- `, `1. `, `[ ] `, `> `, three backticks, `---`,
//!   `**bold**`), undo and redo, selection across blocks, and copy and paste as Markdown.
//!
//! The document model, input, selection, undo and drawing are
//! [gpui-kit-markdown-editor](https://github.com/wabzqem/gpui-kit-markdown-editor)'s
//! (Apache-2.0), through a fork that adds callouts, toggles, block motion and the hooks this
//! crate draws its menus with. It's re-exported as [`markdown_editor`].
//!
//! ```no_run
//! use gpui_kit::*;
//!
//! gpui_kit::application().run(|cx| {
//!     gpui_kit::init(cx);
//!     gpui_blocks::init(cx);
//!     gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
//!         let editor = cx.new(|cx| gpui_blocks::Editor::new("# Hello\n\nType `/`.", window, cx));
//!         editor.update(cx, |editor, cx| editor.focus(window, cx));
//!         editor
//!     })
//!     .unwrap();
//! });
//! ```

pub mod docs;
mod editor;
pub mod json;
pub mod slash;
pub mod toc;
pub mod widget;

pub use editor::{Editor, EditorEvent, Options, extensions};
pub use markdown_editor;
pub use widget::Widget;

// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// The view's actions, to bind to other keys after [`init`]: `gpui_blocks::MoveBlockUp` and so
/// on in a keymap.
pub mod actions {
    gpui_kit::actions!(
        gpui_blocks,
        [
            /// The slash menu's previous item.
            SlashUp,
            /// The slash menu's next item.
            SlashDown,
            /// Runs the slash menu's selected item.
            SlashPick,
            /// Closes the slash menu and leaves what was typed.
            SlashClose,
            /// Moves the caret's block up.
            MoveBlockUp,
            /// Moves the caret's block down.
            MoveBlockDown
        ]
    );
}

/// Sets the editor up: its key bindings and the menus'. Call after `gpui_kit::init`; calling it
/// again does nothing.
pub fn init(cx: &mut gpui_kit::App) {
    struct Initialized;
    impl gpui_kit::Global for Initialized {}
    if cx.has_global::<Initialized>() {
        return;
    }
    cx.set_global(Initialized);
    markdown_editor::init(cx);
    editor::bind_keys(cx);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, Window, div};
    use markdown_editor::{
        BlockEditorModel, BlockKind, BlockPosition, BlockSelection, EditorCommand,
    };

    use crate::widget::{Widget, fenced};

    struct Chart;

    impl Widget for Chart {
        fn name(&self) -> &str {
            "chart"
        }

        fn render(&self, body: &str, _: &mut Window, _: &mut App) -> AnyElement {
            div().child(body.to_string()).into_any_element()
        }
    }

    fn model(source: &str) -> BlockEditorModel {
        let widgets: Vec<Arc<dyn Widget>> = vec![Arc::new(Chart)];
        BlockEditorModel::from_markdown(source, crate::extensions(&widgets)).unwrap()
    }

    fn kinds(m: &BlockEditorModel) -> Vec<BlockKind> {
        m.document().root().children().map(|b| b.kind()).collect()
    }

    #[test]
    fn a_widget_is_one_unit_and_round_trips() {
        let source = "Intro\n\n```chart\nkind: bar\nvalues: 3, 5, 2\n```\n\nAfter";
        let m = model(source);
        assert_eq!(
            kinds(&m),
            [
                BlockKind::Paragraph,
                BlockKind::Custom,
                BlockKind::Paragraph
            ]
        );
        assert_eq!(m.source(), source);
    }

    #[test]
    fn a_widget_inside_a_container_round_trips() {
        for source in [
            "> ```chart\n> a: 1\n> b: 2\n> ```",
            "> [!NOTE]\n> ```chart\n> a: 1\n> ```",
            "- item\n\n  ```chart\n  a: 1\n  ```",
            "```chart title=x\na: 1\n```",
        ] {
            let once = model(source).source();
            assert_eq!(once, source);
            assert_eq!(model(&once).source(), source, "stable after a second save");
        }
    }

    #[test]
    fn other_fenced_code_stays_code() {
        let m = model("```rust\nfn main() {}\n```");
        assert_eq!(kinds(&m), [BlockKind::CodeBlock]);
    }

    #[test]
    fn a_widget_inserted_on_an_empty_line_replaces_it() {
        let mut m = model("Intro\n\n");
        let last = m.document().leaves().count() - 1;
        let id = m.document().leaves().nth(last).unwrap().id();
        m.set_selection(BlockSelection::caret(BlockPosition::new(id, 0)))
            .unwrap();
        m.run(EditorCommand::InsertMarkdown(fenced("chart", "kind: pie")))
            .unwrap();
        assert_eq!(
            kinds(&m),
            [
                BlockKind::Paragraph,
                BlockKind::Custom,
                BlockKind::Paragraph
            ]
        );
        assert!(m.source().contains("```chart\nkind: pie\n```"));
    }

    #[test]
    fn the_sample_reads_back_as_written() {
        // The sample is in the editor's own form, so saving it changes nothing.
        let source = include_str!("../examples/sample.md").trim_end();
        let saved = model(source).source();
        if saved != source {
            std::fs::write(std::env::temp_dir().join("gpui-blocks-sample.md"), &saved).unwrap();
        }
        assert_eq!(saved, source);
    }
}
