//! A code block's chrome, as Notion's: over its top right, while the pointer is over the block or
//! the caret is in it, its language, which opens a menu of common ones, and Copy, which copies
//! its code. A read-only document has Copy alone. Only top-level code blocks have it: the
//! editor knows where those are drawn.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{ActiveTheme as _, Icon, IconName, Popover, PopupMenu, PopupMenuItem};
use markdown_editor::Control as _;
use markdown_editor::{BlockId, BlockKind, BlockPosition, BlockSelection, EditorCommand};

use crate::Editor;

/// The languages the menu offers: the fence's word, and its name. The first is no language.
pub(crate) const LANGUAGES: [(&str, &str); 22] = [
    ("", "Plain text"),
    ("bash", "Bash"),
    ("c", "C"),
    ("cpp", "C++"),
    ("csharp", "C#"),
    ("css", "CSS"),
    ("diff", "Diff"),
    ("go", "Go"),
    ("html", "HTML"),
    ("java", "Java"),
    ("javascript", "JavaScript"),
    ("json", "JSON"),
    ("kotlin", "Kotlin"),
    ("markdown", "Markdown"),
    ("python", "Python"),
    ("ruby", "Ruby"),
    ("rust", "Rust"),
    ("sql", "SQL"),
    ("swift", "Swift"),
    ("toml", "TOML"),
    ("typescript", "TypeScript"),
    ("yaml", "YAML"),
];

/// A language's name as the chrome shows it: the menu's name for a fence word it knows, else
/// the word itself.
fn language_name(lang: &str) -> SharedString {
    LANGUAGES
        .iter()
        .find(|(fence, _)| fence.eq_ignore_ascii_case(lang))
        .map_or_else(|| lang.to_string().into(), |(_, name)| (*name).into())
}

impl Editor {
    /// The code block the chrome is for: the one whose language menu is open, else the
    /// top-level block under the pointer when it's code, else the top-level code block the
    /// caret is in.
    pub(crate) fn chrome_block(&self, cx: &App) -> Option<BlockId> {
        if self.code_menu.is_some() {
            return self.code_menu;
        }
        let state = self.state.read(cx);
        let doc = state.model().document();
        let is_code = |id: BlockId| {
            doc.block(id)
                .is_some_and(|b| b.kind() == BlockKind::CodeBlock)
        };
        if let Some(hovered) = self.hovered.filter(|id| is_code(*id)) {
            return Some(hovered);
        }
        let caret = state.model().caret()?;
        let mut block = doc.block(caret.block)?;
        while let Some(parent) = block.parent()
            && parent.parent().is_some()
        {
            block = parent;
        }
        is_code(block.id()).then(|| block.id())
    }

    /// Gives `block` the language `lang` (empty: none), as one undo step, with the caret in it.
    pub(crate) fn set_code_language(
        &mut self,
        block: BlockId,
        lang: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.code_menu = None;
        self.state.update(cx, |state, cx| {
            if state.is_readonly() {
                return;
            }
            // The command acts on the caret's block.
            let elsewhere = state
                .model()
                .caret()
                .is_none_or(|caret| caret.block != block);
            if elsewhere {
                let _ =
                    state.set_selection(BlockSelection::caret(BlockPosition::new(block, 0)), cx);
            }
            let _ = state.run(
                EditorCommand::SetCodeBlock {
                    lang: Some(lang.to_string()),
                },
                cx,
            );
            state.focus(window, cx);
        });
        cx.notify();
    }

    /// Copies `block`'s code.
    pub(crate) fn copy_code(&mut self, block: BlockId, cx: &mut Context<Self>) {
        let code = self
            .state
            .read(cx)
            .model()
            .document()
            .block(block)
            .and_then(|b| b.text())
            .map(str::to_string);
        if let Some(code) = code {
            cx.write_to_clipboard(ClipboardItem::new_string(code));
        }
    }

    /// The chrome over the code block it's for, if any.
    pub(crate) fn code_chrome(&self, touch: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let block = self.chrome_block(cx)?;
        let state = self.state.read(cx);
        let readonly = state.is_readonly();
        let (_, bounds) = state
            .top_level_blocks()
            .into_iter()
            .find(|(id, _)| *id == block)?;
        let lang = state
            .model()
            .document()
            .block(block)
            .and_then(|b| b.code_language())
            .unwrap_or_default()
            .to_string();
        let theme = cx.theme().clone();
        let chip = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .h(px(if touch { 44. } else { 22. }))
                .when(touch, |d| d.min_w(px(44.)).justify_center())
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(4.))
                .cursor_pointer()
                .text_xs()
                .text_color(theme.muted_foreground)
                .bg(theme.background.opacity(0.85))
                .hover(|s| s.bg(theme.accent))
        };
        let this = cx.entity().downgrade();
        let copy = chip("blocks-code-copy")
            .control(module_path!(), Role::Button, "Copy code")
            .child(
                Icon::new(IconName::Copy)
                    .size(px(12.))
                    .color(theme.muted_foreground),
            )
            .child("Copy")
            .on_mouse_down(MouseButton::Left, {
                let this = this.clone();
                move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = this.update(cx, |this, cx| this.copy_code(block, cx));
                }
            });
        let language = (!readonly).then(|| {
            let current = lang.clone();
            let menu = LANGUAGES.iter().fold(
                PopupMenu::new("blocks-code-language-menu"),
                |menu, (fence, name)| {
                    menu.item(
                        PopupMenuItem::new(format!("lang-{fence}"), *name)
                            .checked(fence.eq_ignore_ascii_case(&current)),
                    )
                },
            );
            let trigger = chip("blocks-code-language")
                .control(module_path!(), Role::Button, "Language")
                .aria_expanded(self.code_menu.is_some())
                .child(if lang.is_empty() {
                    "Plain text".into()
                } else {
                    language_name(&lang)
                })
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(12.))
                        .color(theme.muted_foreground),
                )
                .on_mouse_down(MouseButton::Left, {
                    let this = this.clone();
                    move |_, _, cx| {
                        cx.stop_propagation();
                        let _ = this.update(cx, |this, cx| {
                            this.code_menu = if this.code_menu.is_some() {
                                None
                            } else {
                                Some(block)
                            };
                            cx.notify();
                        });
                    }
                });
            let pick = this.clone();
            let dismiss = this.clone();
            Popover::new("blocks-code-language-popover")
                .open(self.code_menu.is_some())
                .on_dismiss(move |_, _, cx| {
                    let _ = dismiss.update(cx, |this, cx| {
                        this.code_menu = None;
                        cx.notify();
                    });
                })
                .trigger(trigger)
                .p(px(4.))
                .child(menu.on_select(move |picked, window, cx| {
                    let lang = picked
                        .id
                        .strip_prefix("lang-")
                        .unwrap_or_default()
                        .to_string();
                    let _ = pick.update(cx, |this, cx| {
                        this.set_code_language(block, &lang, window, cx)
                    });
                }))
        });
        Some(
            deferred(
                anchored()
                    .position(point(bounds.right() - px(8.), bounds.top() + px(6.)))
                    .anchor(Anchor::TopRight)
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .when_some(language, |d, language| d.child(language))
                            .child(copy),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
