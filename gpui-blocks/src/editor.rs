//! The editor as one view: the document in a centred column, the slash menu at the caret, and
//! the side menu beside the block under the pointer.
//!
//! The document itself is gpui-kit-markdown-editor's (`BlockEditorState`): its model, input,
//! selection, undo and drawing. This view adds what a Notion-like editor has on top, through
//! the editor's public API: the caret's and blocks' bounds to place the menus, `capture_suffix`
//! and `replace_captured` to take back the typed `/…`, `run` for every change, and
//! `begin_block_drag` for the side menu's handle. Nothing here draws on a timer: the menus
//! change on input and pointer moves only.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use markdown_editor::{
    BlockEditor, BlockEditorEvent, BlockEditorState, BlockId, BlockKind, EditOrigin,
    EditorCommand, surfaces::with_default_surfaces,
};

use std::sync::Arc;

use crate::slash::{self, SlashAction, SlashItem};
use crate::widget::{Widget, WidgetPlugin};

actions!(
    gpui_blocks,
    [SlashUp, SlashDown, SlashPick, SlashClose, MoveBlockUp, MoveBlockDown]
);

/// The key context the view adds while the slash menu is open; its keys outrank the editor's.
const SLASH_CONTEXT: &str = "BlocksSlashMenu";

/// The text column's width, as Notion's default page.
const COLUMN: f32 = 720.;
/// Room left of the column for the side menu.
const GUTTER: f32 = 64.;

/// Binds the slash menu's keys. They match only inside the editor while the menu is open, and
/// are bound after the editor's own, so ↑, ↓, Enter, Tab and Esc go to the menu then.
pub fn bind_keys(cx: &mut App) {
    let ctx = Some("BlocksSlashMenu > BlockEditor");
    cx.bind_keys([
        KeyBinding::new("up", SlashUp, ctx),
        KeyBinding::new("down", SlashDown, ctx),
        KeyBinding::new("ctrl-p", SlashUp, ctx),
        KeyBinding::new("ctrl-n", SlashDown, ctx),
        KeyBinding::new("enter", SlashPick, ctx),
        KeyBinding::new("tab", SlashPick, ctx),
        KeyBinding::new("escape", SlashClose, ctx),
        // Notion's keys for moving the caret's block (a list item within its list). They were
        // the editor's for selecting to the document's start and end, which ⌘⇧Home and End
        // still do.
        KeyBinding::new("cmd-shift-up", MoveBlockUp, Some("BlockEditor")),
        KeyBinding::new("cmd-shift-down", MoveBlockDown, Some("BlockEditor")),
    ]);
}

/// The Markdown extensions the editor parses and draws with: the toggles' and callouts'
/// containers, and the widgets.
pub fn extensions(widgets: &[Arc<dyn Widget>]) -> gpui_kit::base::text::MarkdownExtensions {
    widgets.iter().fold(with_default_surfaces(Default::default()), |ext, widget| {
        ext.plugin(WidgetPlugin(widget.clone()))
    })
}

/// What an editor is made with beyond its document: the app's widgets and slash menu items.
#[derive(Clone)]
pub struct Options {
    widgets: Vec<Arc<dyn Widget>>,
    items: Vec<SlashItem>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            items: slash::default_items(),
        }
    }
}

impl Options {
    /// Draw fenced blocks with this widget's name with it (see [`crate::widget`]).
    pub fn widget(mut self, widget: impl Widget) -> Self {
        self.widgets.push(Arc::new(widget));
        self
    }

    /// Add an item to the end of the slash menu.
    pub fn slash_item(mut self, item: SlashItem) -> Self {
        self.items.push(item);
        self
    }

    /// Replace the slash menu's items, the built-in ones included.
    pub fn slash_items(mut self, items: Vec<SlashItem>) -> Self {
        self.items = items;
        self
    }
}

/// What the view tells its owner.
#[derive(Clone, Debug)]
pub enum EditorEvent {
    /// The document changed (once per undoable step, undo and redo included).
    Changed,
}

/// An open slash menu.
#[derive(Clone, Debug, PartialEq)]
struct Slash {
    block: BlockId,
    /// Byte offset of the `/` in the caret's line.
    at: usize,
    query: String,
    matches: Vec<usize>,
    selected: usize,
}

pub struct Editor {
    state: Entity<BlockEditorState>,
    scroll: ScrollHandle,
    menu_scroll: ScrollHandle,
    items: Vec<SlashItem>,
    slash: Option<Slash>,
    /// The `/` closed with Esc. It stays closed until the caret leaves it.
    dismissed: Option<(BlockId, usize)>,
    /// The top-level block the side menu is beside.
    hovered: Option<BlockId>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Editor {
    /// An editor of `markdown` with the built-in blocks and slash menu.
    pub fn new(markdown: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_options(markdown, Options::default(), window, cx)
    }

    /// An editor of `markdown` with an app's widgets and slash menu items.
    pub fn with_options(
        markdown: &str,
        options: Options,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scroll = ScrollHandle::new();
        let widgets = options.widgets.clone();
        let state = cx.new(|cx| {
            let mut state = BlockEditorState::new(window, cx)
                .with_markdown_extensions(extensions(&widgets))
                .with_source(markdown);
            state.set_scroll_handle(scroll.clone());
            state
        });
        let subscription = cx.subscribe(&state, |this, _, event: &BlockEditorEvent, cx| {
            match event {
                BlockEditorEvent::Changed { origin, .. } => {
                    // Typing hides the side menu until the pointer moves again, as Notion's.
                    if matches!(origin, EditOrigin::Typing) && this.hovered.take().is_some() {
                        cx.notify();
                    }
                    this.refresh_slash(cx);
                    cx.emit(EditorEvent::Changed);
                }
                BlockEditorEvent::SelectionChanged => this.refresh_slash(cx),
                BlockEditorEvent::Blur => this.close_slash(cx),
                _ => {}
            }
        });
        Self {
            state,
            scroll,
            menu_scroll: ScrollHandle::new(),
            items: options.items,
            slash: None,
            dismissed: None,
            hovered: None,
            _subscriptions: vec![subscription],
        }
    }

    /// The editor underneath, for what this view doesn't wrap.
    pub fn state(&self) -> &Entity<BlockEditorState> {
        &self.state
    }

    /// The document as Markdown.
    pub fn markdown(&self, cx: &App) -> String {
        self.state.read(cx).source()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let focus = self.state.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// The scroll container the document is in.
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }

    /// Opens, narrows or closes the slash menu from the caret's line.
    fn refresh_slash(&mut self, cx: &mut Context<Self>) {
        let next = {
            let state = self.state.read(cx);
            let model = state.model();
            let caret = model.caret();
            let in_text = matches!(
                model.block_kind_at_caret(),
                Some(BlockKind::Paragraph | BlockKind::Heading { .. })
            );
            match (caret, in_text, state.caret_prefix().and_then(slash::query_at)) {
                (Some(caret), true, Some((at, query))) => {
                    let matches = slash::filter(&self.items, query);
                    if self.dismissed == Some((caret.block, at)) || matches.is_empty() {
                        None
                    } else {
                        let selected = match &self.slash {
                            Some(s) if s.block == caret.block && s.at == at && s.query == query => {
                                s.selected.min(matches.len() - 1)
                            }
                            _ => 0,
                        };
                        Some(Slash {
                            block: caret.block,
                            at,
                            query: query.to_string(),
                            matches,
                            selected,
                        })
                    }
                }
                _ => None,
            }
        };
        if next.is_none() {
            self.dismissed = self.dismissed.filter(|_| self.state_has_slash_at_dismissed(cx));
        }
        if next != self.slash {
            self.slash = next;
            cx.notify();
        }
    }

    /// Whether the caret is still at the `/` that was closed with Esc.
    fn state_has_slash_at_dismissed(&self, cx: &App) -> bool {
        let state = self.state.read(cx);
        let here = state
            .model()
            .caret()
            .zip(state.caret_prefix().and_then(slash::query_at))
            .map(|(caret, (at, _))| (caret.block, at));
        here.is_some() && here == self.dismissed
    }

    fn close_slash(&mut self, cx: &mut Context<Self>) {
        if self.slash.take().is_some() {
            cx.notify();
        }
    }

    fn slash_up(&mut self, _: &SlashUp, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn slash_down(&mut self, _: &SlashDown, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(slash) = &mut self.slash else {
            cx.propagate();
            return;
        };
        let len = slash.matches.len() as isize;
        slash.selected = (slash.selected as isize + by).rem_euclid(len) as usize;
        self.menu_scroll.scroll_to_item(slash.selected);
        cx.notify();
    }

    fn slash_pick(&mut self, _: &SlashPick, window: &mut Window, cx: &mut Context<Self>) {
        match self.slash.as_ref().map(|s| s.matches[s.selected]) {
            Some(ix) => self.pick(ix, window, cx),
            None => cx.propagate(),
        }
    }

    fn move_block(&mut self, down: bool, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let _ = state.run(EditorCommand::MoveBlocks { down }, cx);
        });
    }

    fn slash_close(&mut self, _: &SlashClose, _: &mut Window, cx: &mut Context<Self>) {
        match self.slash.take() {
            Some(slash) => {
                self.dismissed = Some((slash.block, slash.at));
                cx.notify();
            }
            None => cx.propagate(),
        }
    }

    /// Runs an item: the typed `/…` goes, then the line becomes the block.
    fn pick(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slash) = self.slash.take() else {
            return;
        };
        let action = self.items[ix].action.clone();
        let typed = slash.query.len() + 1;
        self.state.update(cx, |state, cx| {
            if let Ok(mut range) = state.capture_suffix(typed) {
                let _ = state.replace_captured(&mut range, "", cx);
            }
            match &action {
                SlashAction::Command(command) => {
                    let _ = state.run(command.clone(), cx);
                }
                SlashAction::Markdown(markdown) => {
                    let _ = state.run(EditorCommand::InsertMarkdown(markdown.to_string()), cx);
                }
                SlashAction::Image => {}
            }
        });
        if action == SlashAction::Image {
            self.insert_image(window, cx);
        }
        self.focus(window, cx);
        cx.notify();
    }

    /// Asks for a picture from this Mac and inserts it as `![name](path)`.
    fn insert_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Insert".into()),
        });
        let state = self.state.clone();
        cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().replace(['[', ']'], ""))
                .unwrap_or_default();
            let markdown = format!("![{name}](<{}>)", path.display());
            let _ = state.update(cx, |state, cx| {
                let _ = state.run(EditorCommand::InsertMarkdown(markdown), cx);
            });
        })
        .detach();
    }

    /// The pointer moved: the side menu goes beside the top-level block level with it.
    fn hover(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button.is_some() {
            return;
        }
        let y = event.position.y;
        let blocks = self.state.read(cx).top_level_blocks();
        let hovered = blocks
            .iter()
            .find(|(_, b)| y >= b.top() - px(6.) && y < b.bottom() + px(6.))
            .map(|(id, _)| *id);
        if hovered != self.hovered {
            self.hovered = hovered;
            cx.notify();
        }
    }

    /// The side menu's "+": a new line below the block with the slash menu open in it, or, on
    /// an empty line, the slash menu there.
    fn add_below(&mut self, block: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let empty = state
                .model()
                .document()
                .block(block)
                .is_some_and(|b| b.kind() == BlockKind::Paragraph && b.text() == Some(""));
            if empty {
                let _ = state.set_selection(
                    markdown_editor::BlockSelection::caret(markdown_editor::BlockPosition::new(block, 0)),
                    cx,
                );
            } else if state.run(EditorCommand::InsertParagraphAfter(block), cx).is_err() {
                return;
            }
            if let Ok(mut range) = state.capture_caret() {
                let _ = state.replace_captured(&mut range, "/", cx);
            }
        });
        self.focus(window, cx);
    }

    fn side_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let block = self.hovered?;
        let state = self.state.read(cx);
        let blocks = state.top_level_blocks();
        let (_, bounds) = blocks.iter().find(|(id, _)| *id == block)?;
        // The column's left edge: where top-level text starts.
        let left = blocks.iter().map(|(_, b)| b.left()).reduce(Pixels::min)?;
        // Level with the block's first line: its height, for one line of the block's kind.
        let line = match state.model().document().block(block).map(|b| b.kind()) {
            Some(BlockKind::Heading { level: 1 }) => px(38.),
            Some(BlockKind::Heading { level: 2 }) => px(32.),
            Some(BlockKind::Heading { level: 3 }) => px(28.),
            _ => px(24.),
        }
        .min(bounds.size.height.max(px(24.)));
        let size = px(22.);
        let top = bounds.top() + (line - size) / 2.;
        let theme = cx.theme().clone();
        let button = |id: &'static str, glyph: &'static str| {
            div()
                .id(id)
                .size(size)
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .text_size(px(15.))
                .hover(|s| s.bg(theme.accent))
                .child(glyph)
        };
        let this = cx.entity().downgrade();
        let this_drag = this.clone();
        let state = self.state.clone();
        Some(
            deferred(
                anchored().position(point(left - px(52.), top)).child(
                    div()
                        .flex()
                        .gap(px(2.))
                        .child(
                            button("blocks-side-add", "+")
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    cx.stop_propagation();
                                    let _ = this.update(cx, |this, cx| this.add_below(block, window, cx));
                                }),
                        )
                        .child(
                            button("blocks-side-grip", "⋮⋮")
                                .cursor_grab()
                                .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                                    // The editor follows the pointer from here and drops the
                                    // block where it's let go, as one undoable move.
                                    cx.stop_propagation();
                                    state.update(cx, |s, _| s.begin_block_drag(block, event.position));
                                    let _ = this_drag.update(cx, |this, cx| {
                                        this.hovered = None;
                                        cx.notify();
                                    });
                                }),
                        ),
                ),
            )
            .with_priority(1),
        )
    }

    fn slash_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let slash = self.slash.as_ref()?;
        let caret = self.state.read(cx).caret_bounds()?;
        let theme = cx.theme();
        let show_groups = slash.query.is_empty();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut group = SharedString::default();
        for (n, &ix) in slash.matches.iter().enumerate() {
            let item = &self.items[ix];
            if show_groups && item.group != group {
                group = item.group.clone();
                rows.push(
                    div()
                        .px_3()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(group.clone())
                        .into_any_element(),
                );
            }
            let selected = n == slash.selected;
            rows.push(
                div()
                    .id(("blocks-slash", ix))
                    .mx_1()
                    .px_2()
                    .py_1()
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(selected, |d| d.bg(theme.accent))
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        if let Some(s) = &mut this.slash
                            && s.selected != n
                        {
                            s.selected = n;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.pick(ix, window, cx);
                        }),
                    )
                    .child(
                        div()
                            .size(px(36.))
                            .flex_none()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_sm()
                            .child(item.icon.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_sm().child(item.title.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .truncate()
                                    .child(item.hint.clone()),
                            ),
                    )
                    .into_any_element(),
            );
        }
        Some(
            deferred(
                anchored()
                    .position(point(caret.left() - px(4.), caret.bottom() + px(6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("blocks-slash-menu")
                            .w(px(300.))
                            .max_h(px(340.))
                            .overflow_y_scroll()
                            .track_scroll(&self.menu_scroll)
                            .py_1()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.popover)
                            .text_color(theme.popover_foreground)
                            .shadow_lg()
                            .children(rows),
                    ),
            )
            .with_priority(2),
        )
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.read(cx).focus_handle(cx)
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side_menu = self.side_menu(cx);
        let slash_menu = self.slash_menu(cx);
        div()
            .id("gpui-blocks")
            .size_full()
            .when(self.slash.is_some(), |d| d.key_context(SLASH_CONTEXT))
            .on_action(cx.listener(Self::slash_up))
            .on_action(cx.listener(Self::slash_down))
            .on_action(cx.listener(Self::slash_pick))
            .on_action(cx.listener(Self::slash_close))
            .on_action(cx.listener(|this, _: &MoveBlockUp, _, cx| this.move_block(false, cx)))
            .on_action(cx.listener(|this, _: &MoveBlockDown, _, cx| this.move_block(true, cx)))
            .on_mouse_move(cx.listener(Self::hover))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                div().w(px(COLUMN + GUTTER * 2.)).max_w_full().mx_auto().min_h_full().child(
                    // Padding on the editor itself, so the whole column takes clicks, and the
                    // gutter on its left holds the side menu.
                    BlockEditor::new(&self.state)
                        .w_full()
                        .min_h_full()
                        .px(px(GUTTER))
                        .pt(px(72.))
                        .pb(px(240.)),
                ),
            )
            .children(side_menu)
            .children(slash_menu)
    }
}

#[cfg(test)]
mod tests {
    use super::Editor;
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use markdown_editor::{BlockKind, BlockPosition, BlockSelection};

    fn open<'a>(cx: &'a mut TestAppContext, source: &str) -> (Entity<Editor>, &'a mut VisualTestContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::init(cx);
        });
        let source = source.to_string();
        let (editor, cx) = cx.add_window_view(move |window, cx| Editor::new(&source, window, cx));
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        (editor, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    fn caret_at_end(editor: &Entity<Editor>, cx: &mut VisualTestContext, leaf: usize) {
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let doc = s.model().document();
                let block = doc.leaves().nth(leaf).unwrap();
                let at = BlockPosition::new(block.id(), block.text().map_or(0, str::len));
                s.set_selection(BlockSelection::caret(at), cx).unwrap();
            })
        });
        draw(cx);
    }

    fn markdown(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
        editor.update(cx, |e, cx| e.markdown(cx))
    }

    fn menu(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
        editor.update(cx, |e, _| {
            e.slash
                .as_ref()
                .map(|s| s.matches.iter().map(|ix| e.items[*ix].title.to_string()).collect())
        })
    }

    #[gpui_kit::test]
    fn slash_opens_narrows_and_picks_with_the_keyboard(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "First line");
        caret_at_end(&editor, cx, 0);
        cx.simulate_keystrokes("enter");
        cx.simulate_input("/");
        draw(cx);
        assert_eq!(menu(&editor, cx).map(|m| m.len()), Some(crate::slash::default_items().len()));
        cx.simulate_input("head");
        draw(cx);
        assert_eq!(menu(&editor, cx).unwrap(), ["Heading 1", "Heading 2", "Heading 3"]);
        // ↓ and Enter are the menu's while it's open, not the editor's.
        cx.simulate_keystrokes("down enter");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
        cx.simulate_input("Plans");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "First line\n\n## Plans");
    }

    #[gpui_kit::test]
    fn escape_closes_the_menu_and_keeps_the_text(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("/to");
        draw(cx);
        assert!(menu(&editor, cx).is_some());
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
        // It stays closed while the same `/` is typed after, and Enter is the editor's again.
        cx.simulate_input("d");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(markdown(&editor, cx).lines().next(), Some("/tod"));
    }

    #[gpui_kit::test]
    fn a_slash_inside_a_word_or_with_no_match_doesnt_open(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("and/or");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
        cx.simulate_input(" /zzz");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
    }

    #[gpui_kit::test]
    fn the_menu_turns_a_line_into_a_toggle_and_a_callout(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("/toggle");
        draw(cx);
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Details");
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "<details open>\n<summary>Details</summary>\n\n</details>"
        );
        cx.simulate_keystrokes("enter enter");
        cx.simulate_input("/callout");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Heads up");
        draw(cx);
        assert!(markdown(&editor, cx).ends_with("</details>\n\n> [!NOTE]\n> Heads up"));
    }

    #[gpui_kit::test]
    fn cmd_shift_arrows_move_the_block_at_the_caret(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "one\n\ntwo\n\n- a\n- b");
        caret_at_end(&editor, cx, 0);
        cx.simulate_keystrokes("cmd-shift-down");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "two\n\none\n\n- a\n- b");
        caret_at_end(&editor, cx, 3);
        cx.simulate_keystrokes("cmd-shift-up");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "two\n\none\n\n- b\n- a");
        cx.simulate_keystrokes("cmd-z cmd-z");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "one\n\ntwo\n\n- a\n- b");
    }

    #[gpui_kit::test]
    fn the_side_menus_plus_adds_a_line_with_the_menu_open(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "# Title\n\nText");
        let first = editor.update(cx, |e, cx| {
            e.state().read(cx).model().document().root().children().next().unwrap().id()
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.add_below(first, window, cx)));
        draw(cx);
        assert!(menu(&editor, cx).is_some());
        cx.simulate_input("quote");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Said");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "# Title\n\n> Said\n\nText");
        let kinds: Vec<BlockKind> = editor.update(cx, |e, cx| {
            e.state().read(cx).model().document().root().children().map(|b| b.kind()).collect()
        });
        assert_eq!(kinds[1], BlockKind::Blockquote);
    }
}
