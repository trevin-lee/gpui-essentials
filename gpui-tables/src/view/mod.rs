//! The views, drawn with GPUI and gpui-core: tables, boards and lists.
//!
//! A [`TableView`] is a GPUI entity holding a database doc, its rows and the active view, which
//! it draws as a board when the view's type is `board`, as a list when it's `list`, and as a
//! table otherwise. It
//! asks its [`TableHost`] for its look ([`TableTheme`]), for names it can't know (people,
//! related rows), for pickers' choices and for a text field, and hands it every change as a
//! [`TableEvent`](crate::TableEvent). Call [`init`] once, before opening a window, for its keys.
//!
//! ```no_run
//! use std::rc::Rc;
//! use gpui::*;
//! use gpui_tables::view::{TableHost, TableTheme, TableView};
//! use gpui_tables::{Doc, TableEvent};
//!
//! struct Host;
//!
//! impl TableHost for Host {
//!     fn theme(&self, cx: &App) -> TableTheme {
//!         TableTheme::from_core(gpui_core::Theme::global(cx))
//!     }
//!
//!     fn event(&self, event: TableEvent, _: &mut Window, _: &mut App) {
//!         println!("{}", serde_json::to_string(&event).unwrap());
//!     }
//! }
//!
//! fn open(database: Doc, rows: Vec<Doc>, cx: &mut App) {
//!     gpui_core::init(cx);
//!     gpui_tables::view::init(cx);
//!     gpui_core::open_window(WindowOptions::default(), cx, |_, cx| {
//!         cx.new(|cx| TableView::new(Rc::new(Host), database, rows, cx))
//!     })
//!     .unwrap();
//! }
//! ```

mod board;
mod board_render;
pub mod format;
mod host;
mod list;
mod list_render;
pub mod ops;
mod render;
mod table;
mod theme;

pub use board::{BoardColumn, BoardSpot, CardGhost};
pub use host::{RelatedRow, TableHost, TextEdit, TextReply, TextTarget};
pub use table::{CellRef, Item, TableView};
pub use theme::{ChipColors, OptionColors, TableColors, TableFonts, TableSizes, TableTheme};

use gpui::{App, KeyBinding, actions};

actions!(
    gpui_tables,
    [
        /// The cell above, or the menu entry above.
        Up,
        /// The cell below, or the menu entry below.
        Down,
        /// The cell to the left (a day back in the date picker).
        Left,
        /// The cell to the right (a day on in the date picker).
        Right,
        /// The next cell, on to the next line after the last.
        NextCell,
        /// The previous cell.
        PrevCell,
        /// Opens a row from its title, edits any other cell, folds a group, makes a row, or
        /// does the highlighted menu entry.
        Confirm,
        /// The highlighted menu entry, adding to what's there (a sort added to the others).
        ConfirmAdd,
        /// Closes a menu, else lets go of the focused cell.
        Cancel,
        /// Empties the focused cell.
        Clear,
        /// Edits the focused cell, the title's too.
        Edit,
        /// Opens the focused cell's column menu.
        ColumnMenu,
        /// A page up (a month back in the date picker).
        PageUp,
        /// A page down (a month on in the date picker).
        PageDown,
    ]
);

/// The key context the table's bindings apply in.
pub const CONTEXT: &str = "GpuiTablesTable";

/// Binds the table's keys: the arrows, Tab and shift-Tab, Enter, Escape, Delete and Backspace,
/// F2 to edit, ⌥↓ for the column's menu, Page Up and Page Down.
pub fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("tab", NextCell, c),
        KeyBinding::new("shift-tab", PrevCell, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("shift-enter", ConfirmAdd, c),
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("backspace", Clear, c),
        KeyBinding::new("delete", Clear, c),
        KeyBinding::new("f2", Edit, c),
        KeyBinding::new("alt-down", ColumnMenu, c),
        KeyBinding::new("pageup", PageUp, c),
        KeyBinding::new("pagedown", PageDown, c),
    ]);
}
