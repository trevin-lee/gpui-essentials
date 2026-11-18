//! The block menu: a click on a block's ⋮⋮ grip (a press that doesn't drag) opens it beside the
//! grip, to turn the block into another kind, colour all its text, duplicate it or delete it,
//! and for a callout to set its icon and colour. On touch, where there's no grip, the long
//! press's edit menu has Block…, which opens the same entries in a sheet.

use gpui::*;
use gpui_core::{ActiveTheme as _, IconName, Popover, PopupMenu, PopupMenuItem, Sheet};
use markdown_editor::{
    BACKGROUNDS, BlockId, BlockPosition, BlockSelection, CALLOUT_COLORS, EditorCommand,
};

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

    /// Copy link to block, when the app makes links to blocks ([`crate::Options::block_link`]).
    /// Open, for a sub-page, an embed or a bookmark: what a click on it does, by keyboard too.
    fn open_item(&self, block: BlockId, cx: &App) -> Option<PopupMenuItem> {
        self.block_target(block, cx)
            .map(|_| PopupMenuItem::new("open", "Open").icon(IconName::ExternalLink))
    }

    /// Resize, for an embed (its height) or a columns block (its widths): the keyboard goes to
    /// the handle, whose arrows then resize it.
    fn resize_item(&self, block: BlockId, cx: &App) -> Option<PopupMenuItem> {
        let doc = self.state.read(cx).model().document();
        let b = doc.block(block)?;
        let label = match b.kind() {
            markdown_editor::BlockKind::Columns => "Resize columns",
            markdown_editor::BlockKind::Custom
                if matches!(
                    crate::json::kind_of_custom(&b.markdown()),
                    crate::json::Kind::Embed { .. }
                ) =>
            {
                "Resize"
            }
            _ => return None,
        };
        Some(PopupMenuItem::new("resize", label).icon(IconName::MoveVertical))
    }

    /// Copy code, for a code block: its chrome's Copy, by keyboard too.
    fn copy_code_item(&self, block: BlockId, cx: &App) -> Option<PopupMenuItem> {
        let doc = self.state.read(cx).model().document();
        (doc.block(block)?.kind() == markdown_editor::BlockKind::CodeBlock)
            .then(|| PopupMenuItem::new("copy-code", "Copy code").icon(IconName::Copy))
    }

    fn copy_link_item(&self) -> Option<PopupMenuItem> {
        self.block_link
            .as_ref()
            .map(|_| PopupMenuItem::new("copy-link", "Copy link to block").icon(IconName::Link))
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
        // Move (the sheet's): the block lifts where it is, for the next finger drag to move.
        if id == "move" {
            self.state.update(cx, |state, cx| {
                state.focus(window, cx);
                state.lift_block(block, window, cx);
            });
            cx.notify();
            return;
        }
        if id == "resize" {
            let columns = self
                .state
                .read(cx)
                .model()
                .document()
                .block(block)
                .is_some_and(|b| b.kind() == markdown_editor::BlockKind::Columns);
            if columns {
                self.resize_columns_by_key(block, window, cx);
            } else {
                self.resize_embed_by_key(block, window, cx);
            }
            return;
        }
        if id == "open" {
            if let Some(event) = self.block_target(block, cx) {
                cx.emit(event);
            }
            self.state.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
            return;
        }
        if id == "copy-code" {
            let code = self
                .state
                .read(cx)
                .model()
                .document()
                .block(block)
                .filter(|b| b.kind() == markdown_editor::BlockKind::CodeBlock)
                .and_then(|b| b.text().map(str::to_string));
            if let Some(code) = code {
                cx.write_to_clipboard(ClipboardItem::new_string(code));
            }
            self.state.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
            return;
        }
        if id == "copy-link" {
            if let Some(link) = self.block_link.clone() {
                let id = self.lock_ids().id_of(block);
                cx.write_to_clipboard(ClipboardItem::new_string(link(&id)));
            }
            self.state.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
            return;
        }
        // A callout's icon or colour: only the menu's own, valid values.
        if let Some(look) = id.strip_prefix("callout-") {
            let (icon, color) = self.callout_look(block, cx).unwrap_or_default();
            let look = match look.split_once('-') {
                Some(("icon", "none")) => Some((None, color)),
                Some(("icon", ix)) => ix
                    .parse::<usize>()
                    .ok()
                    .and_then(|ix| CALLOUT_ICONS.get(ix))
                    .map(|(icon, _)| (Some(SharedString::from(*icon)), color)),
                Some(("color", "none")) => Some((icon, None)),
                Some(("color", name)) => CALLOUT_COLORS
                    .iter()
                    .find(|c| **c == name)
                    .map(|c| (icon, Some(SharedString::from(*c)))),
                _ => None,
            };
            self.state.update(cx, |state, cx| {
                if let Some((icon, color)) = look {
                    let _ = state.run(
                        EditorCommand::SetCalloutLook {
                            callout: block,
                            icon,
                            color,
                        },
                        cx,
                    );
                }
                state.focus(window, cx);
            });
            cx.notify();
            return;
        }
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
        // Move (the desktop menu's): the blocks, selected whole, are picked up by the keyboard.
        if id == "move-keys" {
            self.state.update(cx, |state, cx| {
                state.focus(window, cx);
                if state.set_selection(whole, cx).is_ok() {
                    state.pick_up_blocks(window, cx);
                }
            });
            cx.notify();
            return;
        }
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

    /// `block`'s icon and colour when it's a callout.
    fn callout_look(
        &self,
        block: BlockId,
        cx: &App,
    ) -> Option<(Option<SharedString>, Option<SharedString>)> {
        let state = self.state.read(cx);
        let doc = state.model().document();
        let b = doc.block(block)?;
        b.callout_name()?;
        Some((
            b.callout_icon().map(SharedString::from),
            b.callout_color().map(SharedString::from),
        ))
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
        let turn = turn_into_titles().into_iter().enumerate().fold(
            PopupMenu::new("block-menu-turn"),
            |menu, (ix, title)| {
                let keys = crate::slash::turn_into_keys(&title);
                let item = PopupMenuItem::new(format!("turn-{ix}"), title);
                menu.item(match keys {
                    Some(keys) => item.shortcut(keys),
                    None => item,
                })
            },
        );
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
            .items(self.callout_look(block, cx).map(|(icon, _)| {
                PopupMenuItem::new("callout-icon", "Icon")
                    .icon(IconName::FaceSlightlySmiling)
                    .submenu(icon_entries(
                        PopupMenu::new("block-menu-icon"),
                        icon.as_deref(),
                    ))
            }))
            .items(self.callout_look(block, cx).map(|(_, color)| {
                PopupMenuItem::new("callout-color", "Callout color")
                    .icon(IconName::PaintBucket)
                    .submenu(callout_color_entries(
                        PopupMenu::new("block-menu-callout-color"),
                        cx.theme(),
                        color.as_deref(),
                    ))
            }))
            .items(self.open_item(block, cx))
            .items(self.resize_item(block, cx))
            .items(self.copy_code_item(block, cx))
            .items(self.copy_link_item())
            // Move by keyboard: picked up, the arrows carry it, as Space does with it selected.
            .item(
                PopupMenuItem::new("move-keys", "Move")
                    .icon(IconName::Move)
                    .shortcut("Space"),
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
            .on_dismiss(move |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.close_block_menu(cx);
                    // Back to the page, for a menu opened and closed by keyboard.
                    this.state.update(cx, |state, cx| state.focus(window, cx));
                });
            })
            .trigger(grip)
            .p(px(4.))
            .child(menu)
            .into_any_element()
    }
}

impl Editor {
    /// The block menu as a sheet across the bottom, for touch: Turn into's kinds listed in it
    /// (a submenu has no room beside a phone's menu), Move (the block lifts, for the next finger
    /// drag to move), Duplicate and Delete, then the colours.
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
                .item(PopupMenuItem::new("move", "Move").icon(IconName::Move))
                .items(self.open_item(block, cx))
                .items(self.resize_item(block, cx))
                .items(self.copy_code_item(block, cx))
                .items(self.copy_link_item())
                .item(PopupMenuItem::new("duplicate", "Duplicate").icon(IconName::Copy))
                .item(PopupMenuItem::new("delete", "Delete").icon(IconName::Trash));
            // Its colours last, listed in it as Turn into's are (a sheet has no room for a
            // submenu), so the long list doesn't push Duplicate and Delete out of reach.
            let menu = match self.callout_look(block, cx) {
                Some((icon, color)) => callout_color_entries(
                    icon_entries(menu.separator().heading("Icon"), icon.as_deref())
                        .heading("Callout color"),
                    &theme,
                    color.as_deref(),
                ),
                None => menu,
            };
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

/// The icons the block menu offers a callout, with their names: Notion's usual ones, each a
/// valid callout icon (one emoji).
pub(crate) const CALLOUT_ICONS: [(&str, &str); 16] = [
    ("💡", "Idea"),
    ("📌", "Pin"),
    ("ℹ️", "Information"),
    ("⚠️", "Warning"),
    ("❗", "Important"),
    ("✅", "Done"),
    ("❌", "No"),
    ("❓", "Question"),
    ("📝", "Note"),
    ("🔥", "Fire"),
    ("⭐", "Star"),
    ("🚀", "Launch"),
    ("📣", "Announcement"),
    ("🔒", "Private"),
    ("🐛", "Bug"),
    ("🎯", "Goal"),
];

/// The callout icons as menu entries, `current` checked; "No icon" first.
fn icon_entries(menu: PopupMenu, current: Option<&str>) -> PopupMenu {
    let chip = |icon: &'static str| div().w(px(20.)).flex().justify_center().child(icon);
    menu.item(PopupMenuItem::new("callout-icon-none", "No icon").checked(current.is_none()))
        .items(CALLOUT_ICONS.iter().enumerate().map(|(ix, (icon, name))| {
            PopupMenuItem::new(format!("callout-icon-{ix}"), *name)
                .chip(chip(icon))
                .checked(current == Some(*icon))
        }))
}

/// The callout colours as menu entries with their swatches, `current` checked; "Default"
/// (its kind's own) first.
fn callout_color_entries(
    menu: PopupMenu,
    theme: &gpui_core::Theme,
    current: Option<&str>,
) -> PopupMenu {
    let dark = theme.mode.is_dark();
    let swatch = |fill: Hsla| {
        div()
            .size(px(20.))
            .rounded(px(4.))
            .border_1()
            .border_color(theme.border)
            .bg(fill)
    };
    menu.item(
        PopupMenuItem::new("callout-color-none", "Default")
            .chip(swatch(transparent_black()))
            .checked(current.is_none()),
    )
    .items(
        CALLOUT_COLORS
            .iter()
            .zip(BACKGROUNDS.iter())
            .map(|(name, color)| {
                PopupMenuItem::new(format!("callout-color-{name}"), color.name)
                    .chip(swatch(color.fill(dark)))
                    .checked(current == Some(*name))
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::rust_2024::test;

    #[test]
    fn the_menu_offers_only_valid_looks() {
        for (icon, _) in CALLOUT_ICONS {
            assert!(markdown_editor::callout_icon_ok(icon), "{icon}");
        }
        for (name, color) in CALLOUT_COLORS.iter().zip(BACKGROUNDS.iter()) {
            assert!(color.name.eq_ignore_ascii_case(name), "{name}");
        }
    }
}
