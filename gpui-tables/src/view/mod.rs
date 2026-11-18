//! The views, drawn with GPUI and gpui-core: tables, boards and lists.
//!
//! A [`TableView`] is a GPUI entity holding a database doc, its rows and the active view, which
//! it draws as a board when the view's type is `board`, as a list when it's `list`, and as a
//! table otherwise. It
//! asks its [`TableHost`] for its look ([`TableTheme`]), for names it can't know (people,
//! related rows) and for pickers' choices, edits typed values in place with gpui-core's `Input`
//! (or in the host's own field), and hands the host every change as a
//! [`TableEvent`](crate::TableEvent). Call [`init`] once, before opening a window, for its keys.
//! With [`TableView::with_view_bar`] it draws the database's views as tabs over itself, to
//! switch, add, rename, duplicate, retype, reorder, set up and delete them.
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
mod calc;
mod filter_rules;
pub mod format;
mod gallery;
mod host;
mod list;
mod list_render;
pub mod ops;
mod peek;
mod properties;
mod render;
mod rows;
mod search;
mod table;
mod theme;
mod view_bar;
mod views;

pub use board::{BoardColumn, BoardSpot, CardGhost};
pub use calc::{duration, result_text};
pub use host::{RelatedDatabase, RelatedRow, TableHost, TextEdit, TextReply, TextTarget};
pub use properties::PropertyMenu;
pub use rows::Select;
pub use table::{CellRef, Item, TableView};
pub use theme::{ChipColors, OptionColors, TableColors, TableFonts, TableSizes, TableTheme};
pub use views::ViewMenu;

use gpui::{App, KeyBinding, actions};

actions!(
    gpui_tables,
    [
        /// The cell above. (In a menu or a month, gpui-core's PopupMenu and MonthGrid take the
        /// arrows, Page Up and Page Down, Enter and Escape themselves.)
        Up,
        /// The cell below.
        Down,
        /// The cell to the left.
        Left,
        /// The cell to the right.
        Right,
        /// The next cell, on to the next line after the last.
        NextCell,
        /// The previous cell.
        PrevCell,
        /// Opens a row from its title, edits any other cell, folds a group, makes a row.
        Confirm,
        /// Shift-Enter: as Enter. (In a menu, gpui-core's PopupMenu picks with Shift held, and
        /// a sort picked so is added to the others.)
        ConfirmAdd,
        /// Closes a menu, else lets go of the focused cell.
        Cancel,
        /// Empties the focused cell.
        Clear,
        /// Edits the focused cell, the title's too.
        Edit,
        /// Opens the focused cell's column menu.
        ColumnMenu,
        /// A page up.
        PageUp,
        /// A page down.
        PageDown,
        /// In the bar of views: opens the focused tab's menu (⌥↓).
        TabMenu,
        /// Space: selects the focused row, or lets go of it.
        SelectRow,
        /// Shift-Up: the selection runs up a row.
        SelectUp,
        /// Shift-Down: the selection runs down a row.
        SelectDown,
        /// ⌘⌫: deletes the selected rows, or the focused row (several after asking).
        DeleteRows,
        /// ⌘D: duplicates the selected rows, or the focused row.
        DuplicateRows,
        /// ⌘/: opens the menu of the selected rows, or the focused row.
        RowMenu,
        /// ⌘⇧↑: moves the selected rows (or the focused row) up a place.
        MoveRowsUp,
        /// ⌘⇧↓: moves the selected rows (or the focused row) down a place.
        MoveRowsDown,
        /// ⌘F: opens the bar of views' search field (with the bar on).
        Search,
    ]
);

/// The key context the table's bindings apply in.
pub const CONTEXT: &str = "GpuiTablesTable";

/// The key context of the bar of views ([`TableView::with_view_bar`]): Left and Right move
/// along the tabs, ⌥↓ opens the focused tab's menu, Down and Escape go back to the view.
pub const VIEW_BAR: &str = "GpuiTablesViewBar";

/// ⌘ on the Mac and iOS, Ctrl elsewhere (as gpui-core's fields bind their keys).
macro_rules! cmd {
    ($rest:literal) => {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            concat!("cmd-", $rest)
        } else {
            concat!("ctrl-", $rest)
        }
    };
}

/// Binds the table's keys: the arrows, Tab and shift-Tab, Enter, Escape, Delete and Backspace,
/// F2 to edit, ⌥↓ for the column's menu, Page Up and Page Down; a table's rows' (Space and
/// Shift-arrows to select, ⌘⌫ to delete, ⌘D to duplicate, ⌘/ for their menu, ⌘⇧↑ and ⌘⇧↓ to
/// move); ⌘F to search the rows from the bar; and the bar of views' ([`VIEW_BAR`]). ⌘ is Ctrl
/// off the Mac and iOS.
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
        KeyBinding::new("space", SelectRow, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new(cmd!("backspace"), DeleteRows, c),
        KeyBinding::new(cmd!("d"), DuplicateRows, c),
        KeyBinding::new(cmd!("/"), RowMenu, c),
        KeyBinding::new(cmd!("shift-up"), MoveRowsUp, c),
        KeyBinding::new(cmd!("shift-down"), MoveRowsDown, c),
        KeyBinding::new(cmd!("f"), Search, c),
    ]);
    let b = Some(VIEW_BAR);
    cx.bind_keys([
        KeyBinding::new("left", Left, b),
        KeyBinding::new("right", Right, b),
        KeyBinding::new("down", Down, b),
        KeyBinding::new("escape", Cancel, b),
        KeyBinding::new("alt-down", TabMenu, b),
        KeyBinding::new(cmd!("f"), Search, b),
    ]);
}
