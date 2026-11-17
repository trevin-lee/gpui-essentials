//! The block menu: a click on a block's ⋮⋮ grip (a press that doesn't drag) opens it beside the
//! grip, to turn the block into another kind, duplicate it or delete it.

use gpui::*;
use gpui_core::{IconName, Popover, PopupMenu, PopupMenuItem};
use markdown_editor::{BlockId, BlockPosition, BlockSelection, EditorCommand};

use crate::Editor;
use crate::slash::{self, SlashAction};

impl Editor {
    /// Opens the block menu beside `block`'s grip, as a click on the grip does: for a host's own
    /// way to reach it (a long press on touch, say). `block` is a top-level block.
    pub fn open_block_menu(&mut self, block: BlockId, cx: &mut Context<Self>) {
        self.block_menu = Some(block);
        cx.notify();
    }

    /// The grip of `block` was pressed: the menu opens if it's let go without dragging.
    pub(crate) fn grip_pressed(&mut self, block: BlockId) {
        self.grip_down = Some(block);
    }

    /// The press on a grip ended: a press that didn't start a drag opens the block's menu.
    pub(crate) fn grip_released(&mut self, cx: &mut Context<Self>) {
        let Some(block) = self.grip_down.take() else {
            return;
        };
        if !self.state.read(cx).is_dragging_block() {
            self.block_menu = Some(block);
            cx.notify();
        }
    }

    pub(crate) fn close_block_menu(&mut self, cx: &mut Context<Self>) {
        if self.block_menu.take().is_some() {
            cx.notify();
        }
    }

    /// Runs the block menu's entry `id` on `block`.
    pub(crate) fn block_menu_pick(
        &mut self,
        block: BlockId,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.block_menu = None;
        let whole = BlockSelection::Blocks {
            anchor: block,
            head: block,
        };
        let command = match id {
            "duplicate" => Some((whole, EditorCommand::DuplicateBlocks)),
            "delete" => Some((whole, EditorCommand::DeleteSelection)),
            _ => id
                .strip_prefix("turn-")
                .and_then(|ix| ix.parse::<usize>().ok())
                .and_then(|ix| turn_into_items().into_iter().nth(ix))
                .and_then(|command| Some((self.text_of(block, cx)?, command))),
        };
        if let Some((selection, command)) = command {
            self.state.update(cx, |state, cx| {
                if state.set_selection(selection, cx).is_ok() {
                    let _ = state.run(command, cx);
                }
                state.focus(window, cx);
            });
        }
        cx.notify();
    }

    /// A selection of all of `block`'s text, from its first line's start to its last's end:
    /// `None` for a block with no text (a divider, a widget).
    fn text_of(&self, block: BlockId, cx: &App) -> Option<BlockSelection> {
        let state = self.state.read(cx);
        let doc = state.model().document();
        let inside = |id: BlockId| {
            let mut at = doc.block(id);
            while let Some(b) = at {
                if b.id() == block {
                    return true;
                }
                at = b.parent();
            }
            false
        };
        let leaves: Vec<_> = doc
            .leaves()
            .filter(|leaf| leaf.text().is_some() && inside(leaf.id()))
            .collect();
        let (first, last) = (leaves.first()?, leaves.last()?);
        Some(BlockSelection::Text {
            anchor: BlockPosition::new(first.id(), 0),
            head: BlockPosition::new(last.id(), last.text().map_or(0, str::len)),
        })
    }

    /// The grip of `block`, with the block menu open beside it when it's open for `block`.
    pub(crate) fn grip_with_menu(
        &self,
        block: BlockId,
        grip: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.block_menu != Some(block) {
            return grip;
        }
        let has_text = self.text_of(block, cx).is_some();
        let turn = turn_into_titles()
            .into_iter()
            .enumerate()
            .fold(PopupMenu::new("block-menu-turn"), |menu, (ix, title)| {
                menu.item(PopupMenuItem::new(format!("turn-{ix}"), title))
            });
        let this = cx.entity().downgrade();
        let pick = this.clone();
        let menu = PopupMenu::new("block-menu")
            .item(
                PopupMenuItem::new("turn", "Turn into")
                    .icon(IconName::Repeat)
                    .disabled(!has_text)
                    .submenu(turn),
            )
            .item(
                PopupMenuItem::new("duplicate", "Duplicate")
                    .icon(IconName::Copy)
                    .shortcut(if cfg!(target_os = "macos") {
                        "⌘D"
                    } else {
                        "Ctrl+D"
                    }),
            )
            .separator()
            .item(PopupMenuItem::new("delete", "Delete").icon(IconName::Trash))
            .on_select(move |picked, window, cx| {
                let _ = pick.update(cx, |this, cx| {
                    this.block_menu_pick(block, &picked.id, window, cx)
                });
            });
        Popover::new("block-menu-popover")
            .open(true)
            .on_dismiss(move |_, _, cx| {
                let _ = this.update(cx, |this, cx| this.close_block_menu(cx));
            })
            .trigger(grip)
            .p(px(4.))
            .child(menu)
            .into_any_element()
    }
}

/// What "Turn into" offers: the slash menu's built-in blocks that a command makes of a line.
fn turn_into() -> Vec<(SharedString, EditorCommand)> {
    slash::default_items()
        .into_iter()
        .filter(|item| item.group == slash::BASIC)
        .filter_map(|item| match item.action {
            SlashAction::Command(command) => Some((item.title, command)),
            _ => None,
        })
        .collect()
}

fn turn_into_titles() -> Vec<SharedString> {
    turn_into().into_iter().map(|(title, _)| title).collect()
}

fn turn_into_items() -> Vec<EditorCommand> {
    turn_into()
        .into_iter()
        .map(|(_, command)| command)
        .collect()
}
