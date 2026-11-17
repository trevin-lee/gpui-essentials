//! The block menu: a click on a block's ⋮⋮ grip (a press that doesn't drag) opens it beside the
//! grip, to turn the block into another kind, duplicate it or delete it. On touch, where there's
//! no grip, the long press's edit menu has Block…, which opens the same entries in a sheet.

use gpui::*;
use gpui_core::{IconName, Popover, PopupMenu, PopupMenuItem, Sheet};
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

    /// Selects the top-level blocks from the selection's first block to `block`, whole: what a
    /// Shift-click on `block`'s grip does. Without a selection of top-level blocks, `block` alone.
    pub(crate) fn extend_block_selection(&mut self, block: BlockId, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let doc = state.model().document();
            let top = |id: BlockId| {
                doc.block(id)
                    .is_some_and(|b| b.parent().map(|p| p.id()) == Some(doc.root().id()))
            };
            let anchor = match state.model().selection() {
                BlockSelection::Blocks { anchor, .. } if top(*anchor) => *anchor,
                _ => block,
            };
            let _ = state.set_selection(
                BlockSelection::Blocks {
                    anchor,
                    head: block,
                },
                cx,
            );
        });
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
        if self.block_menu.take().is_some() | self.block_sheet.take().is_some() {
            cx.notify();
        }
    }

    /// Opens the block menu in a sheet for the top-level block the text selection starts in:
    /// the touch edit menu's Block….
    pub(crate) fn open_block_sheet(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let doc = state.model().document();
        let start = match state.model().selection() {
            BlockSelection::Text { anchor, head } => {
                let leaves: Vec<_> = doc.leaves().map(|leaf| leaf.id()).collect();
                let at = |p: &BlockPosition| leaves.iter().position(|id| *id == p.block);
                if at(head) < at(anchor) {
                    head.block
                } else {
                    anchor.block
                }
            }
            _ => return,
        };
        let mut block = doc.block(start);
        while let Some(b) = block
            && b.parent().is_some_and(|p| p.parent().is_some())
        {
            block = b.parent();
        }
        if let Some(block) = block.map(|b| b.id()) {
            self.block_sheet = Some(block);
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
        self.block_sheet = None;
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

impl Editor {
    /// The block menu as a sheet across the bottom, for touch: Turn into's kinds listed in it
    /// (a submenu has no room beside a phone's menu), then Duplicate and Delete. Drawn closed
    /// too, so the focus goes back to the page when it closes.
    pub(crate) fn block_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let block = self.block_sheet;
        let has_text = block.is_some_and(|block| self.text_of(block, cx).is_some());
        let this = cx.entity().downgrade();
        let dismiss = this.clone();
        let menu = block.map(|block| {
            PopupMenu::new("block-sheet-menu")
                .touch(true)
                .heading("Turn into")
                .items(
                    turn_into_titles()
                        .into_iter()
                        .enumerate()
                        .map(|(ix, title)| {
                            PopupMenuItem::new(format!("turn-{ix}"), title).disabled(!has_text)
                        }),
                )
                .separator()
                .item(PopupMenuItem::new("duplicate", "Duplicate").icon(IconName::Copy))
                .item(PopupMenuItem::new("delete", "Delete").icon(IconName::Trash))
                .on_select(move |picked, window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.block_menu_pick(block, &picked.id, window, cx)
                    });
                })
        });
        Sheet::new("block-sheet")
            .open(block.is_some())
            .on_dismiss(move |_, _, cx| {
                let _ = dismiss.update(cx, |this, cx| this.close_block_menu(cx));
            })
            .accessibility_label("Block")
            .children(menu)
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
