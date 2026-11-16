//! The editor as one view: the document in a centred column, the slash menu at the caret, and
//! the side menu beside the block under the pointer.
//!
//! The document itself is gpui-kit-markdown-editor's (`BlockEditorState`): its model, input,
//! selection, undo and drawing. This view adds the menus on top, through the editor's public
//! API: the caret's and blocks' bounds to place them, `capture_suffix` and `replace_captured`
//! to take back the typed `/…`, `run` for every change, and `begin_block_drag` for the side
//! menu's handle. Nothing here draws on a timer: the menus change on input and pointer moves
//! only.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use markdown_editor::{
    BlockEditor, BlockEditorEvent, BlockEditorState, BlockId, BlockKind, BlockPosition,
    BlockSelection, EditOrigin, EditorCommand, surfaces::with_default_surfaces,
};

use std::path::Path;
use std::sync::Arc;

use crate::actions::*;
use crate::docs::{self, DocHooks, Docs, MentionPlugin, PagePlugin};
use crate::slash::{self, SlashAction, SlashItem};
use crate::widget::{Widget, WidgetPlugin};

/// The key context of the whole view.
const CONTEXT: &str = "BlocksEditor";
/// The key contexts while the slash menu is open; the menu's keys outrank the editor's then.
const CONTEXT_WITH_MENU: &str = "BlocksEditor BlocksSlashMenu";

/// The text column's width.
const COLUMN: f32 = 720.;
/// Room left of the column for the side menu.
const GUTTER: f32 = 64.;

/// Binds the slash menu's keys and the keys that move blocks. They match only inside this view
/// (key context `BlocksEditor`, or `BlocksSlashMenu` while the menu is open), and are bound
/// after the editor's own, so ↑, ↓, Enter, Tab and Esc go to the menu while it's open. An app
/// can bind the actions in [`crate::actions`] to other keys after [`crate::init`].
pub(crate) fn bind_keys(cx: &mut App) {
    let menu = Some("BlocksSlashMenu > BlockEditor");
    let editor = Some("BlocksEditor > BlockEditor");
    cx.bind_keys([
        KeyBinding::new("up", SlashUp, menu),
        KeyBinding::new("down", SlashDown, menu),
        KeyBinding::new("ctrl-p", SlashUp, menu),
        KeyBinding::new("ctrl-n", SlashDown, menu),
        KeyBinding::new("enter", SlashPick, menu),
        KeyBinding::new("tab", SlashPick, menu),
        KeyBinding::new("escape", SlashClose, menu),
        // ⌘⇧↑ and ↓ (Ctrl on Linux and Windows) move the caret's block: a list item within its
        // list, anything else among the top-level blocks. Inside this view they replace the
        // editor's select to start and end, which ⌘⇧Home and End still do.
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-up"
            } else {
                "ctrl-shift-up"
            },
            MoveBlockUp,
            editor,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-down"
            } else {
                "ctrl-shift-down"
            },
            MoveBlockDown,
            editor,
        ),
    ]);
}

/// The Markdown extensions the editor parses and draws with: the toggles' and callouts'
/// containers, document links (see [`crate::docs`]) and the widgets.
pub fn extensions(widgets: &[Arc<dyn Widget>]) -> gpui_kit::base::text::MarkdownExtensions {
    extensions_with(widgets, DocHooks::default())
}

fn extensions_with(
    widgets: &[Arc<dyn Widget>],
    hooks: DocHooks,
) -> gpui_kit::base::text::MarkdownExtensions {
    let ext = with_default_surfaces(Default::default())
        .plugin(PagePlugin(hooks.clone()))
        .plugin(MentionPlugin(hooks));
    widgets
        .iter()
        .fold(ext, |ext, widget| ext.plugin(WidgetPlugin(widget.clone())))
}

/// Turns a picture picked from the slash menu's Image item into the Markdown destination to
/// write, or `None` to insert nothing.
type ImageLink = Arc<dyn Fn(&Path) -> Option<String>>;

/// What an editor is made with beyond its document: the app's widgets and slash menu items.
#[derive(Clone)]
pub struct Options {
    widgets: Vec<Arc<dyn Widget>>,
    items: Vec<SlashItem>,
    image_link: Option<ImageLink>,
    images: Option<markdown_editor::ImageSourceFn>,
    docs: Option<Arc<dyn Docs>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            items: slash::default_items(),
            image_link: None,
            images: None,
            docs: None,
        }
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field(
                "widgets",
                &self
                    .widgets
                    .iter()
                    .map(|w| w.name().to_string())
                    .collect::<Vec<_>>(),
            )
            .field("items", &self.items)
            .field("image_link", &self.image_link.is_some())
            .field("images", &self.images.is_some())
            .field("docs", &self.docs.is_some())
            .finish()
    }
}

impl Options {
    /// Draw fenced blocks with this widget's name with it (see [`crate::widget`]). A widget
    /// named after a language, such as `rust`, takes every code block in that language.
    pub fn widget(mut self, widget: impl Widget) -> Self {
        debug_assert!(
            self.widgets.iter().all(|w| w.name() != widget.name()),
            "two widgets are named {:?}",
            widget.name()
        );
        self.widgets.push(Arc::new(widget));
        self
    }

    /// How the slash menu's Image item writes the picture picked from this computer: `link`
    /// returns the link to write (a path relative to the document, or a URL after the app has
    /// copied or uploaded the file), or `None` to insert nothing. Without it the picture's
    /// absolute path is written, which only works on this computer.
    pub fn image_link(mut self, link: impl Fn(&Path) -> Option<String> + 'static) -> Self {
        self.image_link = Some(Arc::new(link));
        self
    }

    /// The app's documents, for the titles and icons of links to them (see [`crate::docs`]).
    /// Adds Page and Link to page to the end of the slash menu.
    pub fn docs(mut self, docs: impl Docs) -> Self {
        self.docs = Some(Arc::new(docs));
        self.items.extend(slash::doc_items());
        self
    }

    /// How pictures load: `load` turns a picture's address, as written in the document, into
    /// the image to draw, from the app's own files, cache or storage, under its own rules about
    /// the network. Without it pictures are drawn as their description, and the editor never
    /// reads a file or fetches an address a document names.
    pub fn images(mut self, load: impl Fn(&str) -> ImageSource + Send + Sync + 'static) -> Self {
        self.images = Some(Arc::new(move |uri: &SharedUri| load(uri.as_ref())));
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
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum EditorEvent {
    /// The document changed: once per edit, undo and redo included.
    Changed,
    /// ⌘K (Ctrl+K on Linux and Windows): the app shows a field for the link on the selection or
    /// at the caret, then calls `set_link` on [`Editor::state`]. `url` is the link there now.
    EditLinkRequested { url: Option<SharedString> },
    /// ⌘-click (Ctrl-click) on a link. The editor never opens links itself.
    LinkClicked { url: SharedString },
    /// A click on a mention or a sub-page (see [`crate::docs`]): the app opens the document.
    DocClicked { id: SharedString },
    /// The slash menu's Page: the app makes a document inside this one, then calls
    /// [`Editor::insert_page`].
    NewPageRequested,
    /// The slash menu's Link to page: the app asks which document, then calls
    /// [`Editor::insert_mention`].
    DocLinkRequested,
}

/// Splits YAML front matter (between `---` lines at the very top) from the document, so the
/// editor never parses it and [`Editor::markdown`] writes it back as it was.
pub(crate) fn split_front_matter(markdown: &str) -> (String, &str) {
    let Some(rest) = markdown
        .strip_prefix("---\n")
        .or_else(|| markdown.strip_prefix("---\r\n"))
    else {
        return (String::new(), markdown);
    };
    let mut at = markdown.len() - rest.len();
    for line in rest.split_inclusive('\n') {
        at += line.len();
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            let front = markdown[..at].replace("\r\n", "\n");
            let front = if front.ends_with('\n') {
                front
            } else {
                front + "\n"
            };
            return (front, markdown[at..].trim_start_matches(['\r', '\n']));
        }
    }
    (String::new(), markdown)
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
    image_link: Option<ImageLink>,
    /// The document's front matter, kept aside (see [`split_front_matter`]).
    front_matter: String,
    slash: Option<Slash>,
    /// The `/` closed with Esc. It stays closed until the caret leaves it.
    dismissed: Option<(BlockId, usize)>,
    /// The next change may open the slash menu even though it wasn't typed (the side menu's
    /// "+" puts a `/` on a new line).
    open_on_next_change: bool,
    /// The top-level block the side menu is beside.
    hovered: Option<BlockId>,
    /// The blocks' ids in [`Editor::document`].
    ids: crate::json::Ids,
    /// A selection handle being dragged by touch: the selection's other end, which stays, and
    /// how far the finger is from the end it moves.
    touch_drag: Option<(BlockPosition, Point<Pixels>)>,
    /// The selection the touch edit menu was used on: it stays hidden until the selection
    /// changes.
    menu_used_on: Option<(BlockPosition, BlockPosition)>,
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
        let (front_matter, body) = split_front_matter(markdown);
        let this = cx.entity().downgrade();
        let hooks = DocHooks {
            docs: options.docs.clone(),
            on_click: Some(Arc::new(move |id: &str, _: &mut Window, cx: &mut App| {
                let id = SharedString::from(id.to_string());
                let _ = this.update(cx, |_, cx| cx.emit(EditorEvent::DocClicked { id }));
            })),
        };
        let images = options.images.clone();
        let state = cx.new(|cx| {
            let mut state = BlockEditorState::new(window, cx)
                .with_markdown_extensions(extensions_with(&widgets, hooks))
                .with_source(body);
            // Pictures load only through the app's loader.
            state = match images {
                Some(load) => state.with_image_source(load),
                None => state.with_image_policy(markdown_editor::ImagePolicy::Placeholder),
            };
            state.set_scroll_handle(scroll.clone());
            state
        });
        let subscription = cx.subscribe(&state, |this, _, event: &BlockEditorEvent, cx| {
            match event {
                BlockEditorEvent::Changed { origin, .. } => {
                    // Typing hides the side menu until the pointer moves again.
                    if matches!(origin, EditOrigin::Typing) && this.hovered.take().is_some() {
                        cx.notify();
                    }
                    let typed = matches!(origin, EditOrigin::Typing | EditOrigin::Backspace);
                    let open = std::mem::take(&mut this.open_on_next_change) || typed;
                    this.refresh_slash(open, cx);
                    cx.emit(EditorEvent::Changed);
                }
                BlockEditorEvent::SelectionChanged => this.refresh_slash(false, cx),
                BlockEditorEvent::Blur => this.close_slash(cx),
                BlockEditorEvent::EditLinkRequested { url } => {
                    cx.emit(EditorEvent::EditLinkRequested { url: url.clone() })
                }
                BlockEditorEvent::LinkClicked { url } => {
                    cx.emit(EditorEvent::LinkClicked { url: url.clone() })
                }
                _ => {}
            }
        });
        Self {
            state,
            scroll,
            menu_scroll: ScrollHandle::new(),
            items: options.items,
            image_link: options.image_link,
            front_matter,
            slash: None,
            dismissed: None,
            open_on_next_change: false,
            hovered: None,
            ids: Default::default(),
            touch_drag: None,
            menu_used_on: None,
            _subscriptions: vec![subscription],
        }
    }

    /// The editor underneath, for what this view doesn't wrap. Its API is
    /// gpui-kit-markdown-editor's and can change with it.
    pub fn state(&self) -> &Entity<BlockEditorState> {
        &self.state
    }

    /// The document as Markdown, its front matter included.
    pub fn markdown(&self, cx: &App) -> String {
        let body = self.state.read(cx).source();
        if self.front_matter.is_empty() {
            body
        } else if body.is_empty() {
            self.front_matter.clone()
        } else {
            format!("{}\n{body}", self.front_matter)
        }
    }

    /// Replaces the document. Undo history starts over, and no [`EditorEvent::Changed`] is sent.
    pub fn set_markdown(&mut self, markdown: &str, cx: &mut Context<Self>) {
        let (front_matter, body) = split_front_matter(markdown);
        self.front_matter = front_matter;
        let _ = self
            .state
            .update(cx, |state, cx| state.set_source(body, cx));
        self.ids.clear();
        self.slash = None;
        self.dismissed = None;
        self.hovered = None;
        cx.notify();
    }

    /// The document as blocks with ids (see [`crate::json`]). A block keeps its id while it's
    /// edited and moved, until the document is replaced; a new block gets a new one.
    pub fn document(&mut self, cx: &App) -> crate::json::Document {
        let state = self.state.read(cx);
        crate::json::Document::from_model(state.model(), self.front_matter.clone(), &mut self.ids)
    }

    /// Replaces the document with `document`, keeping its blocks' ids. Undo history starts over,
    /// and no [`EditorEvent::Changed`] is sent.
    pub fn set_document(&mut self, document: &crate::json::Document, cx: &mut Context<Self>) {
        self.set_markdown(&document.to_markdown(), cx);
        let state = self.state.read(cx);
        self.ids.adopt(state.model(), &document.blocks);
    }

    /// Whether the document can be edited.
    pub fn is_readonly(&self, cx: &App) -> bool {
        self.state.read(cx).is_readonly()
    }

    /// Blocks or allows edits. A read-only document can still be selected and copied, and shows
    /// no side menu.
    pub fn set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_readonly(readonly, cx));
        self.slash = None;
        self.hovered = None;
        cx.notify();
    }

    /// Inserts a sub-page block for the document `id` at the caret (see [`crate::docs`]): on an
    /// empty line in its place, elsewhere after the caret's block. `title` is written as the
    /// link's text, shown while the app has no title for `id`. Does nothing for an invalid id
    /// or a read-only document.
    pub fn insert_page(&mut self, id: &str, title: &str, cx: &mut Context<Self>) {
        self.insert_doc(id, docs::page(id, title), true, cx);
    }

    /// Inserts a mention of the document `id` at the caret, replacing any selection (see
    /// [`crate::docs`]).
    pub fn insert_mention(&mut self, id: &str, title: &str, cx: &mut Context<Self>) {
        self.insert_doc(id, docs::mention(id, title), false, cx);
    }

    fn insert_doc(&mut self, id: &str, markdown: String, block: bool, cx: &mut Context<Self>) {
        if docs::doc_id(&format!("{}{id}", docs::SCHEME)).is_none() {
            return;
        }
        self.state.update(cx, |state, cx| {
            if state.is_readonly() {
                return;
            }
            state.group_edits(cx, |state, cx| {
                if !state.model().selection().is_collapsed() {
                    let _ = state.run(EditorCommand::DeleteSelection, cx);
                }
                // A page goes on a line of its own: this one when it's empty, else a new one
                // after the caret's top-level block.
                if block && !on_empty_line(state) {
                    let Some(top) = state
                        .model()
                        .caret()
                        .and_then(|c| top_level(state, c.block))
                    else {
                        return;
                    };
                    let _ = state.run(EditorCommand::InsertParagraphAfter(top), cx);
                }
                let _ = state.run(EditorCommand::InsertMarkdown(markdown), cx);
            });
        });
    }

    /// Draws the document links again, after the app's documents have changed: renamed, given a
    /// new icon, deleted or shared.
    pub fn refresh_docs(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            // Setting the policy rebuilds every block's drawing, links included.
            let policy = state.image_policy();
            state.set_image_policy(policy, cx);
        });
    }

    /// Gives the editor the keyboard.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let focus = self.state.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// The scroll container the document is in.
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }

    /// Opens, narrows or closes the slash menu from the caret's line. Only typing (or the side
    /// menu's "+") opens it: moving the caret next to a `/` already in the text doesn't.
    fn refresh_slash(&mut self, may_open: bool, cx: &mut Context<Self>) {
        let next = {
            let state = self.state.read(cx);
            let model = state.model();
            let caret = model.caret().filter(|_| model.selection().is_collapsed());
            let in_text = matches!(
                model.block_kind_at_caret(),
                Some(BlockKind::Paragraph | BlockKind::Heading { .. })
            );
            match (
                caret,
                in_text,
                state.caret_prefix().and_then(slash::query_at),
            ) {
                (Some(caret), true, Some((at, query))) => {
                    let matches = slash::filter(&self.items, query);
                    let open = self
                        .slash
                        .as_ref()
                        .is_some_and(|s| s.block == caret.block && s.at == at);
                    if !(may_open || open)
                        || self.dismissed == Some((caret.block, at))
                        || matches.is_empty()
                        || state.is_readonly()
                    {
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
            self.dismissed = self
                .dismissed
                .filter(|_| self.state_has_slash_at_dismissed(cx));
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
        let picked = self
            .slash
            .as_ref()
            .and_then(|s| s.matches.get(s.selected).copied())
            .is_some_and(|ix| self.pick(ix, window, cx));
        if !picked {
            cx.propagate();
        }
    }

    fn move_block(&mut self, down: bool, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let _ = state.run(EditorCommand::MoveBlocks { down }, cx);
        });
    }

    /// Whether the open menu still matches the document: the caret is at the end of the `/…` it
    /// was opened for, with nothing selected and no IME composition in progress. The document
    /// can change under the menu without a `Changed` event (`set_source` sends none).
    fn slash_is_current(&self, slash: &Slash, cx: &App) -> bool {
        let state = self.state.read(cx);
        let model = state.model();
        !state.is_composing()
            && !state.is_readonly()
            && model.selection().is_collapsed()
            && model.caret().is_some_and(|c| c.block == slash.block)
            && state.caret_prefix().and_then(slash::query_at)
                == Some((slash.at, slash.query.as_str()))
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

    /// Runs an item: the typed `/…` goes, then the line becomes the block, as one undo step.
    /// Returns false, and changes nothing, when the menu no longer matches the document.
    fn pick(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(slash) = self.slash.take() else {
            return false;
        };
        cx.notify();
        let current = slash.matches.contains(&ix) && self.slash_is_current(&slash, cx);
        let Some(action) = self
            .items
            .get(ix)
            .map(|i| i.action.clone())
            .filter(|_| current)
        else {
            return false;
        };
        let typed = slash.query.len() + 1;
        let request = match action {
            SlashAction::NewPage => Some(EditorEvent::NewPageRequested),
            SlashAction::LinkToPage => Some(EditorEvent::DocLinkRequested),
            _ => None,
        };
        let image_at = self.state.update(cx, |state, cx| {
            let Ok(mut range) = state.capture_suffix(typed) else {
                return None;
            };
            state.group_edits(cx, |state, cx| {
                state.replace_captured(&mut range, "", cx).ok()?;
                match &action {
                    // A list item picking its own kind of list stays as it is, rather than
                    // toggling the list off.
                    SlashAction::Command(EditorCommand::ToggleList { ordered })
                        if list_at_caret(state) == Some(*ordered) => {}
                    SlashAction::Command(command) => {
                        let _ = state.run(command.clone(), cx);
                    }
                    SlashAction::Markdown(markdown) => {
                        let _ = state.run(EditorCommand::InsertMarkdown(markdown.to_string()), cx);
                    }
                    SlashAction::Image => return state.model().caret(),
                    // The app answers with insert_page or insert_mention.
                    SlashAction::NewPage | SlashAction::LinkToPage => {}
                }
                None
            })
        });
        if let Some(at) = image_at {
            self.insert_image(at, window, cx);
        }
        if let Some(request) = request {
            cx.emit(request);
        }
        self.focus(window, cx);
        true
    }

    /// Asks for a picture from this computer and inserts it at `at` as `![name](link)`.
    fn insert_image(&mut self, at: BlockPosition, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Insert".into()),
        });
        let state = self.state.clone();
        let image_link = self.image_link.clone();
        cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let link = match &image_link {
                Some(link) => link(&path),
                None => Some(path.display().to_string()),
            };
            let Some(markdown) = link.and_then(|link| image_markdown(&path, &link)) else {
                return;
            };
            state.update(cx, |state, cx| {
                // Where the item was picked, if that line is still there.
                let _ = state.set_selection(BlockSelection::caret(at), cx);
                let _ = state.run(EditorCommand::InsertMarkdown(markdown), cx);
            });
        })
        .detach();
    }

    /// The pointer moved: the side menu goes beside the top-level block level with it.
    fn hover(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button.is_some() || self.state.read(cx).is_readonly() {
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
                    markdown_editor::BlockSelection::caret(markdown_editor::BlockPosition::new(
                        block, 0,
                    )),
                    cx,
                );
            } else if state
                .run(EditorCommand::InsertParagraphAfter(block), cx)
                .is_err()
            {
                return;
            }
            if let Ok(mut range) = state.capture_caret() {
                let _ = state.replace_captured(&mut range, "/", cx);
            }
        });
        self.open_on_next_change = true;
        self.focus(window, cx);
    }

    fn side_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let block = self.hovered?;
        let state = self.state.read(cx);
        if state.is_readonly() {
            return None;
        }
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
                                    let _ = this
                                        .update(cx, |this, cx| this.add_below(block, window, cx));
                                }),
                        )
                        .child(
                            button("blocks-side-grip", "⋮⋮")
                                .cursor_grab()
                                .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                                    // The editor follows the pointer from here and drops the
                                    // block where it's let go, as one undoable move.
                                    cx.stop_propagation();
                                    state.update(cx, |s, _| {
                                        s.begin_block_drag(block, event.position)
                                    });
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
                        // Listeners come from the last frame drawn, and typing can narrow the
                        // list before the next one: row `n` may be gone, or hold another item.
                        if let Some(s) = &mut this.slash
                            && s.selected != n
                            && s.matches.get(n) == Some(&ix)
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

/// `Some(ordered)` when the caret's line is a plain (not to-do) list item's first line.
/// Whether the caret is on an empty paragraph, which a block inserted there replaces.
fn on_empty_line(state: &BlockEditorState) -> bool {
    let model = state.model();
    model.block_kind_at_caret() == Some(BlockKind::Paragraph)
        && model
            .caret()
            .and_then(|c| model.document().block(c.block))
            .and_then(|b| b.text())
            == Some("")
}

/// The top-level block `block` is in, or itself.
fn top_level(state: &BlockEditorState, block: BlockId) -> Option<BlockId> {
    let doc = state.model().document();
    let mut at = doc.block(block)?;
    while let Some(parent) = at.parent() {
        if parent.kind() == BlockKind::Root {
            return Some(at.id());
        }
        at = parent;
    }
    None
}

fn list_at_caret(state: &BlockEditorState) -> Option<bool> {
    let model = state.model();
    let line = model.document().block(model.caret()?.block)?;
    let item = line.parent()?;
    if item.kind() != (BlockKind::ListItem { checked: None })
        || item.children().next()?.id() != line.id()
    {
        return None;
    }
    match item.parent()?.kind() {
        BlockKind::List { ordered } => Some(ordered),
        _ => None,
    }
}

/// `![name](<link>)`: the file's name as the picture's text, its Markdown characters escaped,
/// and the link in angle brackets so spaces are allowed. `None` for a link with a line break,
/// which Markdown can't hold.
fn image_markdown(path: &Path, link: &str) -> Option<String> {
    if link.contains(['\n', '\r']) {
        return None;
    }
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut alt = String::new();
    for c in name.chars() {
        if "\\`*_[]<>!".contains(c) {
            alt.push('\\');
        }
        alt.push(c);
    }
    let link = link.replace('<', "\\<").replace('>', "\\>");
    Some(format!("![{alt}](<{link}>)"))
}

impl Focusable for Editor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.read(cx).focus_handle(cx)
    }
}

impl Editor {
    /// On a touch screen, a handle at each end of a text selection: a touch drag from one moves
    /// that end. Shown while the selection was last made or changed by touch.
    fn selection_handles(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let state = self.state.read(cx);
        let BlockSelection::Text { anchor, head } = *state.model().selection() else {
            return None;
        };
        let dragging = self.touch_drag.is_some();
        if anchor == head
            || state.is_composing()
            || !(dragging || window.last_input_was_touch())
            || !state.focus_handle(cx).is_focused(window)
        {
            return None;
        }
        let (a, h) = (state.position_bounds(anchor)?, state.position_bounds(head)?);
        // Which end comes first on the page: the start's knob sits above the line, the end's
        // below, as on iOS.
        let anchor_first = (a.top(), a.left()) <= (h.top(), h.left());
        let ends = if anchor_first {
            [(anchor, a, head, true), (head, h, anchor, false)]
        } else {
            [(head, h, anchor, true), (anchor, a, head, false)]
        };
        let color = cx.theme().primary;
        let knob = px(11.);
        let reach = px(44.);
        let targets: Vec<(Bounds<Pixels>, Point<Pixels>, BlockPosition)> = ends
            .iter()
            .map(|(_, b, other, start)| {
                // A target `reach` wide and tall, from the knob outward: it reaches over the
                // line beside the selection, never far into the selected text, so a press inside
                // the selection is still the text's.
                let x = b.center().x;
                let inside = px(8.);
                let target = if *start {
                    Bounds::from_corners(
                        point(x - reach + inside, b.top() - knob - px(16.)),
                        point(x + inside, b.bottom().max(b.top() - knob - px(16.) + reach)),
                    )
                } else {
                    Bounds::from_corners(
                        point(x - inside, b.top().min(b.bottom() + knob + px(16.) - reach)),
                        point(x + reach - inside, b.bottom() + knob + px(16.)),
                    )
                };
                (target, b.center(), *other)
            })
            .collect();
        let this = cx.entity().downgrade();
        let state_entity = self.state.clone();
        let handles = ends.map(|(_, b, _, start)| {
            let top = if start { b.top() - knob } else { b.top() };
            // In window coordinates, as the bounds are.
            anchored()
                .position(point(b.center().x - knob / 2., top))
                .child(
                    div()
                        .w(knob)
                        .h(b.size.height + knob)
                        .flex()
                        .flex_col()
                        .items_center()
                        .when(start, |d| {
                            d.child(div().size(knob).rounded_full().bg(color))
                        })
                        .child(div().w(px(2.)).h(b.size.height).bg(color))
                        .when(!start, |d| {
                            d.child(div().size(knob).rounded_full().bg(color))
                        })
                        .debug_selector(move || {
                            if start {
                                "blocks-handle-start".into()
                            } else {
                                "blocks-handle-end".into()
                            }
                        }),
                )
        });
        Some(
            deferred(
                div()
                    .children(handles)
                    // Touch drags are offered to the whole window; this takes those that start on
                    // a handle.
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |_, _, window, _| {
                                let this = this.clone();
                                let state = state_entity.clone();
                                let targets = targets.clone();
                                window.on_mouse_event(
                                    move |event: &TouchDragEvent, phase, window, cx| {
                                        if phase != DispatchPhase::Bubble {
                                            return;
                                        }
                                        let _ = this.update(cx, |this, cx| {
                                            this.touch_handle(event, &targets, &state, window, cx)
                                        });
                                    },
                                );
                            },
                        )
                        .size_0(),
                    ),
            )
            .with_priority(2),
        )
    }

    /// On a touch screen, Cut, Copy, Paste and Select all above a text selection (Copy and Select
    /// all when read-only), as iOS shows them. Hidden while a handle moves, and after an item is
    /// used until the selection changes.
    fn edit_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        use markdown_editor::actions::{Copy, Cut, Paste, SelectAll};
        let state = self.state.read(cx);
        let BlockSelection::Text { anchor, head } = *state.model().selection() else {
            return None;
        };
        if anchor == head
            || self.touch_drag.is_some()
            || self.menu_used_on == Some((anchor, head))
            || state.is_composing()
            || !window.last_input_was_touch()
            || !state.focus_handle(cx).is_focused(window)
        {
            return None;
        }
        let (a, h) = (state.position_bounds(anchor)?, state.position_bounds(head)?);
        let (first, last) = if (a.top(), a.left()) <= (h.top(), h.left()) {
            (a, h)
        } else {
            (h, a)
        };
        // Centred over the selection on one row, else over its start; above the start's knob.
        let x = if (first.top() - last.top()).abs() < px(1.) {
            (first.left() + last.left()) / 2.
        } else {
            first.left()
        };
        let at = point(x, first.top() - px(18.));
        let readonly = state.is_readonly();
        let theme = cx.theme();
        let (bg, fg, border) = (theme.popover, theme.popover_foreground, theme.border);
        let this = cx.entity().downgrade();
        let item = move |label: &'static str, action: Box<dyn Action>| {
            let this = this.clone();
            div()
                .id(label)
                .h(px(44.))
                .px(px(14.))
                .flex()
                .items_center()
                .text_size(px(15.))
                .whitespace_nowrap()
                .child(label)
                .debug_selector(move || format!("blocks-menu-{label}"))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    let _ = this.update(cx, |this, cx| this.used_menu(cx));
                    window.dispatch_action(action.boxed_clone(), cx);
                })
        };
        let mut items = Vec::new();
        if !readonly {
            items.push(item("Cut", Box::new(Cut)));
        }
        items.push(item("Copy", Box::new(Copy)));
        if !readonly {
            items.push(item("Paste", Box::new(Paste)));
        }
        items.push(item("Select all", Box::new(SelectAll)));
        Some(
            deferred(
                anchored()
                    .position(at)
                    .anchor(Anchor::BottomCenter)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .flex()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(border)
                            .bg(bg)
                            .text_color(fg)
                            .shadow_md()
                            .overflow_hidden()
                            .debug_selector(|| "blocks-edit-menu".into())
                            .children(items),
                    ),
            )
            .with_priority(3),
        )
    }

    /// A new long press brings the edit menu back, even over the selection it was used on.
    fn long_press_watch(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let this = this.clone();
                window.on_mouse_event(move |event: &LongPressEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.phase == TouchPhase::Started {
                        let _ = this.update(cx, |this, cx| {
                            if this.menu_used_on.take().is_some() {
                                cx.notify();
                            }
                        });
                    }
                });
            },
        )
        .size_0()
    }

    fn used_menu(&mut self, cx: &mut Context<Self>) {
        if let BlockSelection::Text { anchor, head } = *self.state.read(cx).model().selection() {
            self.menu_used_on = Some((anchor, head));
            cx.notify();
        }
    }

    fn touch_handle(
        &mut self,
        event: &TouchDragEvent,
        targets: &[(Bounds<Pixels>, Point<Pixels>, BlockPosition)],
        state: &Entity<BlockEditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.phase {
            TouchPhase::Started => {
                let Some((_, end, other)) = targets
                    .iter()
                    .find(|(target, ..)| target.contains(&event.start_position))
                else {
                    return;
                };
                // Claimed: the touch moves the handle rather than scrolling.
                window.prevent_default();
                cx.stop_propagation();
                self.touch_drag = Some((*other, event.start_position - *end));
                cx.notify();
            }
            TouchPhase::Moved => {
                let Some((other, offset)) = self.touch_drag else {
                    return;
                };
                cx.stop_propagation();
                let aim = event.position - offset;
                state.update(cx, |state, cx| {
                    if let Some(at) = state.position_at(aim)
                        && at != other
                    {
                        let _ = state.set_selection(
                            BlockSelection::Text {
                                anchor: other,
                                head: at,
                            },
                            cx,
                        );
                    }
                });
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.touch_drag.take().is_some() {
                    cx.notify();
                }
            }
        }
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side_menu = self.side_menu(cx);
        let slash_menu = self.slash_menu(cx);
        let handles = self.selection_handles(window, cx);
        let edit_menu = self.edit_menu(window, cx);
        div()
            .id("gpui-blocks")
            .size_full()
            .key_context(if self.slash.is_some() {
                CONTEXT_WITH_MENU
            } else {
                CONTEXT
            })
            .on_action(cx.listener(Self::slash_up))
            .on_action(cx.listener(Self::slash_down))
            .on_action(cx.listener(Self::slash_pick))
            .on_action(cx.listener(Self::slash_close))
            .on_action(cx.listener(|this, _: &MoveBlockUp, _, cx| this.move_block(false, cx)))
            .on_action(cx.listener(|this, _: &MoveBlockDown, _, cx| this.move_block(true, cx)))
            .on_mouse_move(cx.listener(Self::hover))
            // The side menu was placed for the block under the pointer before the scroll; it
            // comes back on the next pointer move.
            .on_scroll(cx.listener(|this, _: &ScrollEvent, _, cx| {
                if this.hovered.take().is_some() {
                    cx.notify();
                }
            }))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                div()
                    .w(px(COLUMN + GUTTER * 2.))
                    .max_w_full()
                    .mx_auto()
                    .min_h_full()
                    .child(
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
            .children(handles)
            .children(edit_menu)
            .child(self.long_press_watch(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::Editor;
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use markdown_editor::{BlockKind, BlockPosition, BlockSelection};
    use std::prelude::rust_2024::test;

    fn open<'a>(
        cx: &'a mut TestAppContext,
        source: &str,
    ) -> (Entity<Editor>, &'a mut VisualTestContext) {
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
            e.slash.as_ref().map(|s| {
                s.matches
                    .iter()
                    .map(|ix| e.items[*ix].title.to_string())
                    .collect()
            })
        })
    }

    #[gpui_kit::test]
    fn slash_opens_narrows_and_picks_with_the_keyboard(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "First line");
        caret_at_end(&editor, cx, 0);
        cx.simulate_keystrokes("enter");
        cx.simulate_input("/");
        draw(cx);
        assert_eq!(
            menu(&editor, cx).map(|m| m.len()),
            Some(crate::slash::default_items().len())
        );
        cx.simulate_input("head");
        draw(cx);
        assert_eq!(
            menu(&editor, cx).unwrap(),
            ["Heading 1", "Heading 2", "Heading 3"]
        );
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
    fn blocks_keep_their_ids_through_edits_moves_and_a_reload(cx: &mut TestAppContext) {
        use crate::json::Kind;
        let (editor, cx) = open(cx, "one\n\ntwo");
        let ids = |editor: &Entity<Editor>, cx: &mut VisualTestContext| -> Vec<(String, String)> {
            editor.update(cx, |e, cx| {
                e.document(cx)
                    .blocks
                    .into_iter()
                    .map(|b| match b.kind {
                        Kind::Paragraph { text } => (text, b.id),
                        other => (format!("{other:?}"), b.id),
                    })
                    .collect()
            })
        };
        let first = ids(&editor, cx);
        assert_eq!(ids(&editor, cx), first, "asking again changes nothing");
        // Typing keeps the block's id.
        caret_at_end(&editor, cx, 0);
        cx.simulate_input("!");
        draw(cx);
        let typed = ids(&editor, cx);
        assert_eq!(typed[0], ("one!".into(), first[0].1.clone()));
        // Moving keeps both.
        cx.simulate_keystrokes("cmd-shift-down");
        draw(cx);
        let moved = ids(&editor, cx);
        assert_eq!(moved, [typed[1].clone(), typed[0].clone()]);
        // A new line gets a new id.
        cx.simulate_keystrokes("enter");
        cx.simulate_input("three");
        draw(cx);
        let added = ids(&editor, cx);
        assert_eq!(added.len(), 3);
        assert!(added[2].1 != moved[0].1 && added[2].1 != moved[1].1);
        // Stored and loaded again, every block has the id it had.
        let stored = editor.update(cx, |e, cx| e.document(cx).to_json());
        editor.update(cx, |e, cx| e.set_markdown("something else", cx));
        let doc = crate::json::Document::from_json(&stored).unwrap();
        editor.update(cx, |e, cx| e.set_document(&doc, cx));
        assert_eq!(ids(&editor, cx), added);
    }

    /// The middle of the caret box at `offset` in the first leaf.
    fn point_at(
        editor: &Entity<Editor>,
        cx: &mut VisualTestContext,
        offset: usize,
    ) -> gpui_kit::Point<gpui_kit::Pixels> {
        editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            let id = state.model().document().leaves().next().unwrap().id();
            state
                .position_bounds(BlockPosition::new(id, offset))
                .expect("laid out")
                .center()
        })
    }

    fn selected(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> (usize, usize) {
        editor.update(cx, |e, cx| match e.state().read(cx).selection() {
            BlockSelection::Text { anchor, head } => (anchor.offset, head.offset),
            other => panic!("{other:?}"),
        })
    }

    fn touch(
        cx: &mut VisualTestContext,
        phase: gpui_kit::TouchPhase,
        at: gpui_kit::Point<gpui_kit::Pixels>,
    ) {
        cx.simulate_event(gpui_kit::TouchEvent {
            id: gpui_kit::TouchId(7),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        draw(cx);
    }

    /// Drags the selection handle `name` by touch to `to`, from its knob.
    fn drag_handle(
        cx: &mut VisualTestContext,
        name: &'static str,
        to: gpui_kit::Point<gpui_kit::Pixels>,
    ) {
        use gpui_kit::TouchPhase::*;
        let handle = cx.debug_bounds(name).expect("the handle is drawn");
        let knob = if name.ends_with("start") {
            gpui_kit::point(handle.center().x, handle.top() + gpui_kit::px(5.))
        } else {
            gpui_kit::point(handle.center().x, handle.bottom() - gpui_kit::px(5.))
        };
        let offset = knob - handle.center();
        touch(cx, Started, knob);
        let halfway = gpui_kit::point((knob.x + to.x + offset.x) / 2., knob.y);
        touch(cx, Moved, halfway);
        touch(cx, Moved, to + offset);
        touch(cx, Ended, to + offset);
    }

    #[gpui_kit::test]
    fn touch_handles_move_the_ends_of_a_selection(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world again");
        let word = point_at(&editor, cx, 8);
        // A finger held on a word: GPUI recognizes the long press after a moment.
        touch(cx, gpui_kit::TouchPhase::Started, word);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        draw(cx);
        touch(cx, gpui_kit::TouchPhase::Ended, word);
        assert_eq!(selected(&editor, cx), (6, 11));
        assert!(cx.debug_bounds("blocks-handle-start").is_some());
        // The end's handle out to the end of the line.
        let end = point_at(&editor, cx, 17);
        drag_handle(cx, "blocks-handle-end", end);
        assert_eq!(selected(&editor, cx), (6, 17));
        // The start's back to the start.
        let start = point_at(&editor, cx, 0);
        drag_handle(cx, "blocks-handle-start", start);
        assert_eq!(selected(&editor, cx), (17, 0));
        assert_eq!(markdown(&editor, cx), "hello world again");
    }

    fn long_press(cx: &mut VisualTestContext, at: gpui_kit::Point<gpui_kit::Pixels>) {
        touch(cx, gpui_kit::TouchPhase::Started, at);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        draw(cx);
        touch(cx, gpui_kit::TouchPhase::Ended, at);
    }

    fn tap(cx: &mut VisualTestContext, name: &'static str) {
        let at = cx
            .debug_bounds(name)
            .unwrap_or_else(|| panic!("{name} isn't drawn"))
            .center();
        touch(cx, gpui_kit::TouchPhase::Started, at);
        touch(cx, gpui_kit::TouchPhase::Ended, at);
    }

    #[gpui_kit::test]
    fn the_touch_menu_copies_pastes_and_selects_all(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world again");
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        assert!(cx.debug_bounds("blocks-edit-menu").is_some());
        tap(cx, "blocks-menu-Copy");
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some("world")
        );
        assert_eq!(
            selected(&editor, cx),
            (6, 11),
            "copying keeps the selection"
        );
        assert!(
            cx.debug_bounds("blocks-edit-menu").is_none(),
            "and closes the menu"
        );
        // Selected again, then pasted over.
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("planet".into()));
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        tap(cx, "blocks-menu-Paste");
        assert_eq!(markdown(&editor, cx), "hello planet again");
        let at = point_at(&editor, cx, 2);
        long_press(cx, at);
        tap(cx, "blocks-menu-Select all");
        assert_eq!(selected(&editor, cx), (0, 18));
    }

    #[gpui_kit::test]
    fn a_read_only_touch_menu_only_copies(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world");
        editor.update(cx, |e, cx| e.set_readonly(true, cx));
        draw(cx);
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        assert!(cx.debug_bounds("blocks-menu-Copy").is_some());
        assert!(cx.debug_bounds("blocks-menu-Cut").is_none());
        assert!(cx.debug_bounds("blocks-menu-Paste").is_none());
    }

    #[gpui_kit::test]
    fn a_selection_made_with_a_mouse_has_no_handles(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world");
        let word = point_at(&editor, cx, 8);
        cx.simulate_click(word, gpui_kit::Modifiers::none());
        cx.simulate_event(gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Left,
            position: word,
            modifiers: gpui_kit::Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(gpui_kit::MouseUpEvent {
            button: gpui_kit::MouseButton::Left,
            position: word,
            modifiers: gpui_kit::Modifiers::none(),
            click_count: 2,
        });
        draw(cx);
        assert_eq!(selected(&editor, cx), (6, 11));
        assert!(cx.debug_bounds("blocks-handle-start").is_none());
    }

    #[gpui_kit::test]
    fn pictures_load_only_through_the_apps_loader(cx: &mut TestAppContext) {
        use std::sync::{Arc, Mutex};
        let source = "Before ![a picture](https://example.test/p.png) after";
        // Without a loader, pictures are their description.
        let (editor, cx) = open(cx, source);
        let policy = editor.update(cx, |e, cx| e.state().read(cx).image_policy());
        assert_eq!(policy, markdown_editor::ImagePolicy::Placeholder);
        // With one, it's asked for the address.
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = asked.clone();
        let options = crate::Options::default().images(move |address| {
            seen.lock().unwrap().push(address.to_string());
            gpui_kit::ImageSource::Image(Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Svg,
                b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'/>".to_vec(),
            )))
        });
        let (_, cx) =
            cx.add_window_view(move |window, cx| Editor::with_options(source, options, window, cx));
        draw(cx);
        assert!(
            asked
                .lock()
                .unwrap()
                .iter()
                .any(|a| a == "https://example.test/p.png")
        );
    }

    #[gpui_kit::test]
    fn a_stale_selection_in_the_menu_never_panics(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("/");
        draw(cx);
        // As a pointer move from the frame before the list narrowed could leave it.
        cx.simulate_input("h1");
        editor.update(cx, |e, _| e.slash.as_mut().unwrap().selected = 9);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
    }

    #[gpui_kit::test]
    fn moving_the_caret_next_to_a_slash_doesnt_open_the_menu(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "read / write\n\nnext");
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let id = s.model().document().leaves().next().unwrap().id();
                s.set_selection(BlockSelection::caret(BlockPosition::new(id, 6)), cx)
                    .unwrap();
            })
        });
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "read /\n\n write\n\nnext");
    }

    #[gpui_kit::test]
    fn a_menu_left_over_from_a_replaced_document_does_nothing(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("/h");
        draw(cx);
        assert!(menu(&editor, cx).is_some());
        editor.update(cx, |e, cx| {
            e.state()
                .update(cx, |s, cx| s.set_source("Hello world", cx).unwrap())
        });
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(!markdown(&editor, cx).starts_with('#'));
    }

    #[gpui_kit::test]
    fn a_pick_undoes_in_one_step(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("/head");
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "# ");
        cx.simulate_keystrokes("cmd-z");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "/head");
    }

    #[gpui_kit::test]
    fn picking_a_lists_own_kind_keeps_the_item(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "- item");
        caret_at_end(&editor, cx, 0);
        cx.simulate_input(" /bullet");
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "- item ");
    }

    #[gpui_kit::test]
    fn front_matter_is_kept_aside_and_written_back(cx: &mut TestAppContext) {
        let source = "---\ntitle: Notes\ntags: [a]\n---\n\n# Notes";
        let (editor, cx) = open(cx, source);
        assert_eq!(markdown(&editor, cx), source);
        let shown = editor.update(cx, |e, cx| e.state().read(cx).source());
        assert_eq!(shown, "# Notes");
        editor.update(cx, |e, cx| e.set_markdown("---\na: 1\n---\nBody", cx));
        assert_eq!(markdown(&editor, cx), "---\na: 1\n---\n\nBody");
    }

    #[test]
    fn front_matter_needs_its_closing_line() {
        assert_eq!(
            super::split_front_matter("---\nnot closed"),
            (String::new(), "---\nnot closed")
        );
        assert_eq!(
            super::split_front_matter("# Title\n---\n"),
            (String::new(), "# Title\n---\n")
        );
        assert_eq!(
            super::split_front_matter("---\r\na: 1\r\n---\r\n\r\nBody"),
            ("---\na: 1\n---\n".to_string(), "Body")
        );
    }

    #[test]
    fn a_pictures_name_and_link_are_escaped() {
        let path = std::path::Path::new("/tmp/my_*pic*.png");
        assert_eq!(
            super::image_markdown(path, "/tmp/a <b>.png").unwrap(),
            "![my\\_\\*pic\\*](</tmp/a \\<b\\>.png>)"
        );
        assert_eq!(super::image_markdown(path, "a\nb"), None);
    }

    #[gpui_kit::test]
    fn dividers_and_widgets_get_a_side_menu_too(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "a\n\n---\n\nb");
        let blocks = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks().len());
        assert_eq!(blocks, 3);
    }

    #[gpui_kit::test]
    fn cmd_shift_arrows_in_a_toggles_body_keep_its_summary(cx: &mut TestAppContext) {
        let (editor, cx) = open(
            cx,
            "<details open>\n<summary>Title</summary>\n\nBody\n\n</details>",
        );
        caret_at_end(&editor, cx, 1);
        cx.simulate_keystrokes("cmd-shift-up");
        draw(cx);
        assert!(markdown(&editor, cx).contains("<summary>Title</summary>"));
    }

    #[gpui_kit::test]
    fn cmd_shift_arrows_in_a_table_keep_its_columns(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "| h1 | h2 |\n| --- | --- |\n| a | b |");
        caret_at_end(&editor, cx, 3);
        cx.simulate_keystrokes("cmd-shift-up");
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "| h1 | h2 |\n| --- | --- |\n| a | b |"
        );
    }

    #[gpui_kit::test]
    fn the_side_menus_plus_adds_a_line_with_the_menu_open(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "# Title\n\nText");
        let first = editor.update(cx, |e, cx| {
            e.state()
                .read(cx)
                .model()
                .document()
                .root()
                .children()
                .next()
                .unwrap()
                .id()
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
            e.state()
                .read(cx)
                .model()
                .document()
                .root()
                .children()
                .map(|b| b.kind())
                .collect()
        });
        assert_eq!(kinds[1], BlockKind::Blockquote);
    }
}
