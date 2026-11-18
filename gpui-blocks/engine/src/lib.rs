// Changed in Trevin Lee's fork (2026): the reorder and surfaces modules, toggle surface names,
// the clipboard actions exported, the extension module.
//! A block-based WYSIWYG Markdown editor for GPUI, drawn with gpui-core.
//!
//! The design began as a spike in
//! [longbridge/gpui-kit#3049](https://github.com/longbridge/gpui-kit/pull/3049)
//! and [#3566](https://github.com/longbridge/gpui-kit/pull/3566); see
//! `docs/design.md`. Call [`init`] once, after `gpui_core::init`.
//!
//! - [`BlockEditorModel`]: the headless model. It has stable [`BlockId`]s,
//!   validated [`BlockPosition`]s, normalized inline mark runs, invertible
//!   steps grouped into transactions, undo coalescing, commands on nested
//!   blocks ([`EditorCommand`]), and a Markdown serializer.
//! - [`BlockEditorState`]: the entity that owns a model, focus, IME and
//!   key bindings, and emits [`BlockEditorEvent`]s.
//! - [`BlockEditor`]: the view. It draws the blocks on gpui-core's paragraph layer and culls
//!   off-screen blocks.
//!
//! ```ignore
//! let scroll = ScrollHandle::new();
//! let editor = cx.new(|cx| {
//!     let mut state = BlockEditorState::new(window, cx).with_source("# Hello");
//!     // Optional: keep the caret in view inside your scroll container.
//!     state.set_scroll_handle(scroll.clone());
//!     state
//! });
//! cx.subscribe(&editor, |_, editor, event: &BlockEditorEvent, cx| {
//!     if let BlockEditorEvent::Changed { .. } = event {
//!         let markdown = editor.read(cx).source();
//!     }
//! });
//! // in render:
//! div().id("doc").overflow_y_scroll().track_scroll(&scroll)
//!     .child(BlockEditor::new(&editor))
//! ```
//!
//! Keys: Enter, Backspace/Delete, arrows/Home/End (+Shift), Cmd/Ctrl+A, B, I,
//! E, Z, Shift+Z, C, X, V, and Shift+V (plain paste); Tab/Shift+Tab indent
//! list items; Cmd/Ctrl+Alt+0 makes the block (or each selected block) a
//! paragraph. Input rules: `# `..`###### `, `- `, `* `, `+ `, `1. `.
//! The scroll handle is optional: without it, blocks outside the visible
//! area are still culled, but moving the caret never scrolls (a no-op).

// Resolve existing GPUI paths and macro expansions through the Kit facade.

mod command;
mod cull;
mod document;
mod draw;
mod editor;
mod element;
pub mod extension;
mod find;
mod frame;
mod host;
mod id;
mod import;
mod inline;
mod input_rule;
mod kind;
mod markdown_source;
mod marks;
mod order;
mod position;
mod projection;
mod reorder;
mod selection;
mod serialize;
mod slice;
mod state;
mod step;
mod style;
mod support;
pub(crate) mod table;
mod table_ui;
mod text_mark;
mod transaction;
mod undo;

#[cfg(test)]
mod tests;

pub mod surfaces;

pub use command::{CommandError, EditorCommand};
pub use document::{BlockRef, DocumentRef};
pub use editor::BlockEditorModel;
pub use element::BlockEditor;
pub use extension::{CustomNode, Extension, Extensions, InlineContext};
pub use find::FindMatch;
/// The Markdown parser's syntax tree for `source`, or why it has none: a parser panic too (see
/// its docs), so no input brings the app down.
pub use import::parse as parse_markdown;
pub use input_rule::InputRule;
pub use kind::ColumnAlign;
pub use state::{
    BlockEditorEvent, BlockEditorState, ClipboardPolicy, CodeHighlightFn, ImageSourceFn,
    PasteRequest, SubmitPolicy,
};
/// The editor's clipboard and selection actions, for a host's own menu (an edit menu over a touch
/// selection, say) to dispatch to the focused editor.
pub mod actions {
    pub use crate::state::{Copy, Cut, EditLink, Paste, PastePlain, SelectAll};
}
pub use style::BlockEditorStyle;
pub use table::{TableEdit, TableLine};

/// Registers the editor's key bindings. Call once at startup, after `gpui_core::init`.
pub fn init(cx: &mut gpui::App) {
    state::init(cx);
}
/// gpui-core first (its theme and keys), then the editor.
#[cfg(test)]
pub(crate) fn init_for_tests(cx: &mut gpui::App) {
    gpui_core::init(cx);
    init(cx);
}

pub use id::BlockId;
pub use kind::BlockKind;
pub use marks::{
    BACKGROUNDS, MarkChange, MarkKind, MarkSet, MarkState, PaletteColor, background_of,
};
pub use position::{Affinity, BlockPosition, PositionError};
pub use selection::{BlockSelection, SelectionError};
pub use transaction::{EditOrigin, Transaction};

pub use host::{CapturedRange, CapturedRangeError, SourceSnapshot};

/// Whether image atoms may load resources while the editor presents them.
/// `Placeholder` keeps the atomic document, source and selection but never
/// resolves URLs, decodes data, reads paths or creates image elements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImagePolicy {
    #[default]
    Load,
    Placeholder,
}
