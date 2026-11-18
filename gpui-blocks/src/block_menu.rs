//! The block menu: a click on a block's ⋮⋮ grip (a press that doesn't drag) opens it beside the
//! grip, to turn the block into another kind, colour all its text, duplicate it or delete it. On touch, where there's
//! no grip, the long press's edit menu has Block…, which opens the same entries in a sheet.

use gpui::*;
use gpui_core::{ActiveTheme as _, IconName, Popover, PopupMenu, PopupMenuItem, Sheet};
use markdown_editor::{BlockId, BlockPosition, BlockSelection, EditorCommand};

use crate::Editor;
use crate::format_bar::{Paint, color_entries};
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
        if let Some(block) = self.selection_top_block(cx) {
            self.block_sheet = Some(block);
            cx.notify();
        }
    }

    /// The top-level block the text selection starts in.
    pub(crate) fn selection_top_block(&self, cx: &App) -> Option<BlockId> {
        let state = self.state.read(cx);
        let doc = state.model().document();
        let BlockSelection::Text { anchor, head } = state.model().selection() else {
            return None;
        };
        let leaves: Vec<_> = doc.leaves().map(|leaf| leaf.id()).collect();
        let at = |p: &BlockPosition| leaves.iter().position(|id| *id == p.block);
        let start = if at(head) < at(anchor) {
            head.block
        } else {
            anchor.block
        };
        let mut block = doc.block(start);
        while let Some(b) = block
            && b.parent().is_some_and(|p| p.parent().is_some())
        {
            block = b.parent();
        }
        block.map(|b| b.id())
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
        let (first, last) = self.menu_blocks(block, cx);
        // Color: the blocks' text, all of it, as Notion colours a block.
        if let Some((paint, on)) = Paint::of_menu_item(id) {
            if let Some(selection) = self.text_of(first, last, cx) {
                let selected = self
                    .state
                    .update(cx, |state, cx| state.set_selection(selection, cx).is_ok());
                if selected {
                    self.set_paint(paint, on, cx);
                }
            }
            self.state.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
            return;
        }
        let whole = BlockSelection::Blocks {
            anchor: first,
            head: last,
        };
        let command = match id {
            "duplicate" => Some((whole, EditorCommand::DuplicateBlocks)),
            "delete" => Some((whole, EditorCommand::DeleteSelection)),
            _ => id
                .strip_prefix("turn-")
                .and_then(|ix| ix.parse::<usize>().ok())
                .and_then(|ix| turn_into_items().into_iter().nth(ix))
                .and_then(|command| Some((self.text_of(first, last, cx)?, command))),
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

    /// The blocks `block`'s menu acts on, first and last: the selected blocks when `block` is
    /// one of them, as Notion's does, else `block` alone. A text selection's blocks are the
    /// top-level ones it runs through (the format bar's Turn into).
    pub(crate) fn menu_blocks(&self, block: BlockId, cx: &App) -> (BlockId, BlockId) {
        let state = self.state.read(cx);
        let doc = state.model().document();
        let top = |id: BlockId| {
            let mut at = doc.block(id)?;
            while let Some(parent) = at.parent()
                && parent.parent().is_some()
            {
                at = parent;
            }
            Some(at.id())
        };
        let ends = match *state.model().selection() {
            BlockSelection::Blocks { anchor, head } => Some((anchor, head)),
            BlockSelection::Text { anchor, head } => top(anchor.block).zip(top(head.block)),
            _ => None,
        };
        let parent = |id| doc.block(id).and_then(|b| b.parent());
        if let Some((anchor, head)) = ends
            && let Some(siblings) = parent(block)
            && parent(anchor).map(|p| p.id()) == Some(siblings.id())
            && parent(head).map(|p| p.id()) == Some(siblings.id())
        {
            let ids: Vec<_> = siblings.children().map(|b| b.id()).collect();
            let at = |id| ids.iter().position(|b| *b == id);
            if let (Some(a), Some(h), Some(b)) = (at(anchor), at(head), at(block)) {
                let (lo, hi) = (a.min(h), a.max(h));
                if (lo..=hi).contains(&b) {
                    return (ids[lo], ids[hi]);
                }
            }
        }
        (block, block)
    }

    /// A selection of the text of the blocks `first` to `last` (siblings, or the same block),
    /// from the first line's start to the last's end: `None` when they have no text (a
    /// divider, a widget).
    fn text_of(&self, first: BlockId, last: BlockId, cx: &App) -> Option<BlockSelection> {
        let state = self.state.read(cx);
        let doc = state.model().document();
        let blocks: Vec<BlockId> = match doc.block(first).and_then(|b| b.parent()) {
            Some(parent) if first != last => {
                let ids: Vec<_> = parent.children().map(|b| b.id()).collect();
                let at = |id| ids.iter().position(|b| *b == id);
                let (a, b) = (at(first)?, at(last)?);
                ids[a.min(b)..=a.max(b)].to_vec()
            }
            _ => vec![first],
        };
        let inside = |id: BlockId| {
            let mut at = doc.block(id);
            while let Some(b) = at {
                if blocks.contains(&b.id()) {
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
        let (first, last) = self.menu_blocks(block, cx);
        let has_text = self.text_of(first, last, cx).is_some();
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
                PopupMenuItem::new("color", "Color")
                    .icon(IconName::Palette)
                    .disabled(!has_text)
                    .submenu(color_entries(
                        PopupMenu::new("block-menu-color"),
                        cx.theme(),
                        None,
                    )),
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
    /// (a submenu has no room beside a phone's menu), Duplicate and Delete, then the colours.
    /// Drawn closed too, so the focus goes back to the page when it closes.
    pub(crate) fn block_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let block = self.block_sheet;
        let has_text = block.is_some_and(|block| {
            let (first, last) = self.menu_blocks(block, cx);
            self.text_of(first, last, cx).is_some()
        });
        let this = cx.entity().downgrade();
        let dismiss = this.clone();
        let theme = cx.theme().clone();
        let menu = block.map(|block| {
            let menu = PopupMenu::new("block-sheet-menu")
                .touch(true)
                .heading("Turn into")
                .items(
                    turn_into_titles()
                        .into_iter()
                        .enumerate()
                        .map(|(ix, title)| {
                            PopupMenuItem::new(format!("turn-{ix}"), title).disabled(!has_text)
                        }),
                );
            let menu = menu
                .separator()
                .item(PopupMenuItem::new("duplicate", "Duplicate").icon(IconName::Copy))
                .item(PopupMenuItem::new("delete", "Delete").icon(IconName::Trash));
            // Its colours last, listed in it as Turn into's are (a sheet has no room for a
            // submenu), so the long list doesn't push Duplicate and Delete out of reach.
            let menu = if has_text {
                color_entries(menu.separator(), &theme, None)
            } else {
                menu
            };
            menu.on_select(move |picked, window, cx| {
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

pub(crate) fn turn_into_titles() -> Vec<SharedString> {
    turn_into().into_iter().map(|(title, _)| title).collect()
}

fn turn_into_items() -> Vec<EditorCommand> {
    turn_into()
        .into_iter()
        .map(|(_, command)| command)
        .collect()
}
