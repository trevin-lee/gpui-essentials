//! The editor as one view: the document in a centred column, the slash menu at the caret, and
//! the side menu beside the block under the pointer.
//!
//! The document itself is gpui-kit-markdown-editor's (`BlockEditorState`): its model, input,
//! selection, undo and drawing. This view adds the menus on top, through the editor's public
//! API: the caret's and blocks' bounds to place them, `capture_suffix` and `replace_captured`
//! to take back the typed `/…`, `run` for every change, and `begin_block_drag` for the side
//! menu's handle. Nothing here draws on a timer: the menus change on input and pointer moves
//! only.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::ActiveTheme as _;
use markdown_editor::{
    BlockEditor, BlockEditorEvent, BlockEditorState, BlockId, BlockKind, BlockPosition,
    BlockSelection, EditOrigin, EditorCommand, Extensions,
};

use std::path::Path;
use std::sync::Arc;

use crate::actions::*;
use crate::docs::{
    self, DocHooks, Docs, Embed, EmbedFn, EmbedHooks, EmbedPlugin, MentionPlugin, PagePlugin,
    ResizeStart,
};
use crate::mentions::{ChipPlugin, MentionHooks, MentionKind, MentionMenu, Mentions};
use crate::slash::{self, SlashAction, SlashItem};
use crate::toc::{Heading, TocHooks, TocPlugin};
use crate::widget::{Widget, WidgetPlugin};

/// The key context of the whole view.
const CONTEXT: &str = "BlocksEditor";
/// The key contexts while the slash menu is open; the menu's keys outrank the editor's then.
const CONTEXT_WITH_MENU: &str = "BlocksEditor BlocksSlashMenu";

/// The text column's width.
const COLUMN: f32 = 720.;
/// Room each side of the column, the left one holding the side menu ([`Options::gutter`]).
const GUTTER: f32 = 64.;
/// Room above the first block ([`Options::top`]).
const TOP: f32 = 72.;
/// Below this width the editor lays itself out for a phone ([`Options::compact_below`]).
const COMPACT_BELOW: f32 = 600.;
/// The compact layout's room each side of the column and above it.
const COMPACT_INSET: f32 = 16.;

/// Binds the slash menu's keys and the keys that move blocks. They match only inside this view
/// (key context `BlocksEditor`, or `BlocksSlashMenu` while the menu is open), and are bound
/// after the editor's own, so ↑, ↓, Enter, Tab and Esc go to the menu while it's open. An app
/// can bind the actions in [`crate::actions`] to other keys after [`crate::init`].
pub(crate) fn bind_keys(cx: &mut App) {
    let menu = Some("BlocksSlashMenu > BlockEditor");
    let editor = Some("BlocksEditor > BlockEditor");
    let mac = cfg!(target_os = "macos");
    // Find works from the page and from the find bar.
    for page in ["BlocksEditor", crate::find::CONTEXT] {
        cx.bind_keys([
            KeyBinding::new(if mac { "cmd-f" } else { "ctrl-f" }, FindOpen, Some(page)),
            KeyBinding::new(
                if mac { "cmd-alt-f" } else { "ctrl-h" },
                FindReplaceOpen,
                Some(page),
            ),
            KeyBinding::new(if mac { "cmd-g" } else { "f3" }, FindNext, Some(page)),
            KeyBinding::new(
                if mac { "cmd-shift-g" } else { "shift-f3" },
                FindPrevious,
                Some(page),
            ),
        ]);
    }
    cx.bind_keys([KeyBinding::new(
        "escape",
        FindClose,
        Some(crate::find::CONTEXT),
    )]);
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
pub fn extensions(widgets: &[Arc<dyn Widget>]) -> Extensions {
    extensions_with(
        widgets,
        DocHooks::default(),
        TocHooks::default(),
        EmbedHooks::default(),
        MentionHooks::default(),
    )
}

fn extensions_with(
    widgets: &[Arc<dyn Widget>],
    hooks: DocHooks,
    toc: TocHooks,
    embeds: EmbedHooks,
    chips: MentionHooks,
) -> Extensions {
    let ext = Extensions::default()
        .with(TocPlugin(toc))
        .with(EmbedPlugin(embeds))
        .with(PagePlugin(hooks.clone()))
        .with(MentionPlugin(hooks))
        .with(ChipPlugin(chips));
    widgets
        .iter()
        .fold(ext, |ext, widget| ext.with(WidgetPlugin(widget.clone())))
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
    highlighter: Option<markdown_editor::CodeHighlightFn>,
    docs: Option<Arc<dyn Docs>>,
    embeds: Option<Arc<EmbedFn>>,
    mentions: Option<Arc<dyn Mentions>>,
    gutter: Pixels,
    top: Pixels,
    compact_below: Pixels,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            items: slash::default_items(),
            image_link: None,
            images: None,
            highlighter: None,
            docs: None,
            embeds: None,
            mentions: None,
            gutter: px(GUTTER),
            top: px(TOP),
            compact_below: px(COMPACT_BELOW),
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
            .field("highlighter", &self.highlighter.is_some())
            .field("docs", &self.docs.is_some())
            .field("embeds", &self.embeds.is_some())
            .field("mentions", &self.mentions.is_some())
            .field("gutter", &self.gutter)
            .field("top", &self.top)
            .field("compact_below", &self.compact_below)
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

    /// How code blocks are highlighted: `highlight` gets a block's language (the first word of
    /// its fence's info, possibly empty) and its code, and returns the styles of byte ranges of
    /// the code, sorted. gpui-core's `highlight` feature makes one
    /// (`Highlighter::new().into_fn(theme)`). It's asked again only when a block's code or
    /// language changes; pass a new one when the theme changes. Without it, code blocks are
    /// drawn uncoloured.
    pub fn code_highlighter(
        mut self,
        highlight: impl Fn(&str, &str) -> Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.highlighter = Some(Arc::new(highlight));
        self
    }

    /// How embedded documents are drawn (see [`crate::docs`]): `draw` gets the document, the
    /// view the embed asks for and its height, and returns its content, or `None` for the
    /// editor's placeholder. The editor draws the frame, the title and the handle that drags
    /// the height.
    pub fn embeds(
        mut self,
        draw: impl Fn(&Embed, &mut Window, &mut App) -> Option<AnyElement> + Send + Sync + 'static,
    ) -> Self {
        self.embeds = Some(Arc::new(draw));
        self
    }

    /// Who and what `@` can mention: the app's people, agents, pages and today's date (see
    /// [`crate::mentions`]). Without it, `@` is an ordinary character.
    pub fn mentions(mut self, mentions: impl Mentions) -> Self {
        self.mentions = Some(Arc::new(mentions));
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

    /// The room each side of the text column, 64 by default. The left one holds the side menu
    /// (the + and the grip), which needs about 56.
    pub fn gutter(mut self, gutter: Pixels) -> Self {
        self.gutter = gutter;
        self
    }

    /// The room above the first block, 72 by default.
    pub fn top(mut self, top: Pixels) -> Self {
        self.top = top;
        self
    }

    /// Below this width (600 by default) the editor lays itself out for a phone: 16 each side
    /// of the column and above it, in place of [`Options::gutter`] and [`Options::top`], and no
    /// side menu, which has no room there. `px(0.)` keeps the wide layout at any width.
    pub fn compact_below(mut self, width: Pixels) -> Self {
        self.compact_below = width;
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
    /// The slash menu's Embed: the app asks which document and view, then calls
    /// [`Editor::insert_embed`].
    EmbedRequested,
    /// A click on a person, agent or date mention (see [`crate::mentions`]): the app shows it.
    /// A page's mention sends [`EditorEvent::DocClicked`].
    MentionClicked { kind: MentionKind, id: SharedString },
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
pub(crate) struct Slash {
    block: BlockId,
    /// Byte offset of the `/` in the caret's line.
    at: usize,
    query: String,
    matches: Vec<usize>,
    selected: usize,
}

pub struct Editor {
    pub(crate) state: Entity<BlockEditorState>,
    /// The find bar, when it's open.
    pub(crate) find: Option<crate::find::FindBar>,
    /// The block whose grip is pressed, until it's let go (see `block_menu.rs`).
    pub(crate) grip_down: Option<BlockId>,
    /// The block whose menu is open.
    pub(crate) block_menu: Option<BlockId>,
    /// The block whose menu is open in a sheet, from the touch edit menu's Block….
    pub(crate) block_sheet: Option<BlockId>,
    /// Whether the format bar's Turn into menu is open.
    pub(crate) format_turn: bool,
    scroll: ScrollHandle,
    pub(crate) menu_scroll: ScrollHandle,
    /// [`Options::gutter`], [`Options::top`] and [`Options::compact_below`].
    gutter: Pixels,
    top: Pixels,
    compact_below: Pixels,
    items: Vec<SlashItem>,
    image_link: Option<ImageLink>,
    /// The document's front matter, kept aside (see [`split_front_matter`]).
    front_matter: String,
    pub(crate) slash: Option<Slash>,
    /// The app's people, agents and pages, for the @ menu.
    pub(crate) mentions: Option<Arc<dyn Mentions>>,
    /// The open @ menu.
    pub(crate) mention: Option<MentionMenu>,
    /// The `@` closed with Esc. It stays closed until the caret leaves it.
    pub(crate) mention_dismissed: Option<(BlockId, usize)>,
    /// The `/` closed with Esc. It stays closed until the caret leaves it.
    dismissed: Option<(BlockId, usize)>,
    /// The next change may open the slash menu even though it wasn't typed (the side menu's
    /// "+" puts a `/` on a new line).
    open_on_next_change: bool,
    /// The top-level block the side menu is beside.
    hovered: Option<BlockId>,
    /// Embeds' heights while one is being resized.
    embed_heights: docs::Heights,
    /// The embed being resized.
    resizing: Option<ResizeStart>,
    /// A document link pasted on a line of its own, which can become a sub-page or an embed.
    paste_offer: Option<PasteOffer>,
    /// The app's documents, for the title of a pasted `doc:` address.
    docs: Option<Arc<dyn Docs>>,
    /// The headings a table of contents lists, kept up to date after each change.
    headings: Arc<std::sync::Mutex<Vec<Heading>>>,
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
        let highlighter = options.highlighter.clone();
        let embed_heights: docs::Heights = Arc::default();
        let this = cx.entity().downgrade();
        let that = this.clone();
        let embeds = EmbedHooks {
            docs: options.docs.clone(),
            render: options.embeds.clone(),
            on_click: Some(Arc::new(move |id: &str, _: &mut Window, cx: &mut App| {
                let id = SharedString::from(id.to_string());
                let _ = that.update(cx, |_, cx| cx.emit(EditorEvent::DocClicked { id }));
            })),
            heights: embed_heights.clone(),
            on_resize: Some(Arc::new(move |start, _: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |this, cx| {
                    this.resizing = Some(start);
                    cx.notify();
                });
            })),
        };
        let headings: Arc<std::sync::Mutex<Vec<Heading>>> = Arc::default();
        let this = cx.entity().downgrade();
        let chips = MentionHooks {
            on_click: Some(Arc::new(
                move |kind: MentionKind, id: &str, _: &mut Window, cx: &mut App| {
                    let id = SharedString::from(id.to_string());
                    let _ = this.update(cx, |_, cx| {
                        cx.emit(EditorEvent::MentionClicked { kind, id })
                    });
                },
            )),
        };
        let this = cx.entity().downgrade();
        let toc = TocHooks {
            headings: headings.clone(),
            on_click: Some(Arc::new(move |block, window: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |this, cx| this.go_to(block, window, cx));
            })),
        };
        let state = cx.new(|cx| {
            let mut state = BlockEditorState::new(window, cx)
                .with_markdown_extensions(extensions_with(&widgets, hooks, toc, embeds, chips))
                .with_source(body);
            // Pictures load only through the app's loader.
            state = match images {
                Some(load) => state.with_image_source(load),
                None => state.with_image_policy(markdown_editor::ImagePolicy::Placeholder),
            };
            if let Some(highlighter) = highlighter {
                state = state.with_code_highlighter(highlighter);
            }
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
                    this.update_headings(cx);
                    this.refresh_paste_offer(*origin == EditOrigin::Paste, cx);
                    this.refresh_slash(open, cx);
                    this.refresh_mention(open, cx);
                    cx.emit(EditorEvent::Changed);
                }
                BlockEditorEvent::SelectionChanged => {
                    this.keep_paste_offer(cx);
                    this.refresh_slash(false, cx);
                    this.refresh_mention(false, cx);
                }
                BlockEditorEvent::Blur => {
                    this.close_slash(cx);
                    this.close_mention(cx);
                }
                BlockEditorEvent::EditLinkRequested { url } => {
                    cx.emit(EditorEvent::EditLinkRequested { url: url.clone() })
                }
                BlockEditorEvent::LinkClicked { url } => {
                    cx.emit(EditorEvent::LinkClicked { url: url.clone() })
                }
                _ => {}
            }
        });
        let editor = Self {
            state,
            scroll,
            menu_scroll: ScrollHandle::new(),
            gutter: options.gutter,
            top: options.top,
            compact_below: options.compact_below,
            items: options.items,
            image_link: options.image_link,
            front_matter,
            slash: None,
            mentions: options.mentions.clone(),
            mention: None,
            mention_dismissed: None,
            dismissed: None,
            open_on_next_change: false,
            hovered: None,
            ids: Default::default(),
            headings,
            embed_heights,
            resizing: None,
            paste_offer: None,
            docs: options.docs.clone(),
            touch_drag: None,
            find: None,
            grip_down: None,
            block_menu: None,
            block_sheet: None,
            format_turn: false,
            menu_used_on: None,
            _subscriptions: vec![subscription],
        };
        editor.update_headings(cx);
        editor
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
        self.update_headings(cx);
        self.slash = None;
        self.dismissed = None;
        self.mention = None;
        self.mention_dismissed = None;
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

    /// What a comment on the selection is anchored to: the lasting id (as in
    /// [`Editor::document`]) of the block the caret is in, or the selection starts in, and the
    /// selected text, empty for a caret. Inline pictures and mentions are left out of the text, and
    /// lines of several blocks are joined with `\n`. `None` for a selection of whole blocks or
    /// table cells.
    pub fn selection_anchor(&mut self, cx: &App) -> Option<(String, String)> {
        let state = self.state.read(cx);
        let BlockSelection::Text { anchor, head } = state.model().selection().clone() else {
            return None;
        };
        let doc = state.model().document();
        let leaves: Vec<_> = doc.leaves().collect();
        let place = |p: &BlockPosition| {
            leaves
                .iter()
                .position(|l| l.id() == p.block)
                .map(|ix| (ix, p.offset))
        };
        let (a, h) = (place(&anchor)?, place(&head)?);
        let (start, end) = if a <= h { (a, h) } else { (h, a) };
        let mut lines = Vec::new();
        for (ix, leaf) in leaves.iter().enumerate().take(end.0 + 1).skip(start.0) {
            let text = leaf.text().unwrap_or_default();
            let from = if ix == start.0 {
                start.1.min(text.len())
            } else {
                0
            };
            let to = if ix == end.0 {
                end.1.min(text.len())
            } else {
                text.len()
            };
            lines.push(
                text.get(from..to)
                    .unwrap_or_default()
                    .replace('\u{FFFC}', ""),
            );
        }
        let block = leaves[start.0].id();
        Some((self.ids.id_of(block), lines.join("\n")))
    }

    /// Where the top-level block holding the block `id` (a lasting id, as in
    /// [`Editor::document`]) was last drawn, in window coordinates: for a mark in the margin
    /// beside it. `None` for an id the document doesn't have, or a block scrolled out of view.
    pub fn block_bounds(&self, id: &str, cx: &App) -> Option<Bounds<Pixels>> {
        let state = self.state.read(cx);
        let mut block = state
            .model()
            .document()
            .block(self.ids.block_of(state.model(), id)?)?;
        while let Some(parent) = block.parent()
            && parent.parent().is_some()
        {
            block = parent;
        }
        let top = block.id();
        state
            .top_level_blocks()
            .into_iter()
            .find(|(id, _)| *id == top)
            .map(|(_, bounds)| bounds)
    }

    /// Puts the caret at the start of the block `id` (a lasting id, as in [`Editor::document`])
    /// and scrolls to it, with the editor focused: for going to what a comment is about. `false`
    /// for an id the document doesn't have.
    pub fn reveal_block(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let state = self.state.clone();
        let Some(block) = self.ids.block_of(state.read(cx).model(), id) else {
            return false;
        };
        state.update(cx, |state, cx| {
            let doc = state.model().document();
            let inside = |leaf: markdown_editor::BlockRef<'_>| {
                let mut at = Some(leaf);
                while let Some(b) = at {
                    if b.id() == block {
                        return true;
                    }
                    at = b.parent();
                }
                false
            };
            let Some(leaf) = doc
                .leaves()
                .find(|leaf| inside(*leaf))
                .map(|leaf| leaf.id())
            else {
                return false;
            };
            let moved = state
                .set_selection(BlockSelection::caret(BlockPosition::new(leaf, 0)), cx)
                .is_ok();
            state.reveal_caret(cx);
            state.focus(window, cx);
            moved
        })
    }

    /// Makes every embed of the document `doc` that shows the view `from` show `to` instead,
    /// keeping its title and height, as one undo step: for remembering, with the page, the view
    /// someone switched to inside an embedded database. `to` follows [`docs::embed`]'s rule for
    /// views. Returns how many embeds changed; none in a read-only editor.
    pub fn set_embed_view(
        &mut self,
        doc: &str,
        from: &str,
        to: &str,
        cx: &mut Context<Self>,
    ) -> usize {
        self.state.update(cx, |state, cx| {
            if state.is_readonly() {
                return 0;
            }
            fn walk(block: markdown_editor::BlockRef<'_>, out: &mut Vec<(BlockId, String)>) {
                if block.kind() == BlockKind::Custom {
                    out.push((block.id(), block.markdown()));
                }
                for child in block.children() {
                    walk(child, out);
                }
            }
            let mut found = Vec::new();
            walk(state.model().document().root(), &mut found);
            let changes: Vec<(BlockId, String)> = found
                .into_iter()
                .filter_map(|(block, markdown)| {
                    let (id, title, view, height) = docs::embed_of(&markdown)?;
                    (id == doc && view == from)
                        .then(|| (block, docs::embed(&id, &title, to, height)))
                })
                .collect();
            state.group_edits(cx, |state, cx| {
                changes
                    .into_iter()
                    .filter(|(block, markdown)| {
                        state
                            .run(
                                EditorCommand::ReplaceBlock {
                                    block: *block,
                                    markdown: markdown.clone(),
                                },
                                cx,
                            )
                            .is_ok()
                    })
                    .count()
            })
        })
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
        self.mention = None;
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

    /// After a paste, offers to turn a line holding only a document link (a mention, or the text
    /// `doc:id`) into a sub-page or an embed. Any other change closes the offer.
    fn refresh_paste_offer(&mut self, pasted: bool, cx: &mut Context<Self>) {
        let offer = pasted
            .then(|| paste_offer(&self.state, self.docs.as_deref(), cx))
            .flatten();
        if offer.is_some() || self.paste_offer.is_some() {
            self.paste_offer = offer;
            cx.notify();
        }
    }

    /// Closes the offer when the caret leaves its line.
    fn keep_paste_offer(&mut self, cx: &mut Context<Self>) {
        let Some(offer) = &self.paste_offer else {
            return;
        };
        let here = self.state.read(cx).model().caret().map(|c| c.block);
        if here != Some(offer.block) {
            self.paste_offer = None;
            cx.notify();
        }
    }

    /// Turns the offered line into `kind`, as one undo step.
    fn take_paste_offer(&mut self, kind: PasteAs, cx: &mut Context<Self>) {
        let Some(offer) = self.paste_offer.take() else {
            return;
        };
        cx.notify();
        if kind == PasteAs::Mention && !offer.plain {
            return;
        }
        let len = self
            .state
            .read(cx)
            .model()
            .document()
            .block(offer.block)
            .and_then(|b| b.text())
            .map(str::len);
        let Some(len) = len else {
            return;
        };
        self.state.update(cx, |state, cx| {
            let _ = state.set_selection(
                BlockSelection::Text {
                    anchor: BlockPosition::new(offer.block, 0),
                    head: BlockPosition::new(offer.block, len),
                },
                cx,
            );
        });
        match kind {
            PasteAs::Mention => self.insert_mention(&offer.doc, &offer.title, cx),
            PasteAs::Page => self.insert_page(&offer.doc, &offer.title, cx),
            PasteAs::Embed => self.insert_embed(&offer.doc, &offer.title, docs::DEFAULT_VIEW, cx),
        }
    }

    /// The offer's menu, under the pasted line.
    fn paste_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let offer = self.paste_offer.as_ref()?;
        let state = self.state.read(cx);
        if state.is_readonly() {
            return None;
        }
        let caret = state.caret_bounds()?;
        let theme = cx.theme();
        let (bg, fg, muted, border, accent) = (
            theme.popover,
            theme.popover_foreground,
            theme.muted_foreground,
            theme.border,
            theme.accent,
        );
        let this = cx.entity().downgrade();
        let mut kinds = Vec::new();
        if offer.plain {
            kinds.push((PasteAs::Mention, "Mention"));
        }
        kinds.push((PasteAs::Page, "Page"));
        kinds.push((PasteAs::Embed, "Embed"));
        let items = kinds.into_iter().map(move |(kind, label)| {
            let this = this.clone();
            div()
                .id(label)
                .debug_selector(move || format!("blocks-paste-{label}"))
                .px(px(10.))
                .py(px(5.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(move |s| s.bg(accent))
                .child(label)
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = this.update(cx, |this, cx| this.take_paste_offer(kind, cx));
                })
        });
        Some(
            deferred(
                anchored()
                    .position(point(caret.left(), caret.bottom() + px(6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .p(px(4.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(border)
                            .bg(bg)
                            .text_color(fg)
                            .text_size(px(13.))
                            .shadow_md()
                            .child(div().px(px(6.)).text_color(muted).child("Show as"))
                            .children(items),
                    ),
            )
            .with_priority(2),
        )
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

    /// Inserts an embed of the document `id` at the caret (see [`crate::docs`]), shown as `view`
    /// at the default height, on a line of its own as [`Editor::insert_page`] does.
    pub fn insert_embed(&mut self, id: &str, title: &str, view: &str, cx: &mut Context<Self>) {
        let markdown = docs::embed(id, title, view, docs::DEFAULT_HEIGHT);
        self.insert_doc(id, markdown, true, cx);
    }

    /// Follows the pointer while an embed's height is dragged.
    fn resize_to(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let Some(start) = &self.resizing else {
            return;
        };
        let height = (start.embed.height as f32 + f32::from(y - start.at.y)).round();
        let height = (height.max(0.) as u32).clamp(*docs::HEIGHTS.start(), *docs::HEIGHTS.end());
        if let Ok(mut heights) = self.embed_heights.lock() {
            heights.insert(start.key, height);
        }
        cx.notify();
    }

    /// Writes the dragged height into the document, as one undo step.
    fn finish_resize(&mut self, cx: &mut Context<Self>) {
        let Some(start) = self.resizing.take() else {
            return;
        };
        let height = self
            .embed_heights
            .lock()
            .ok()
            .and_then(|mut h| h.remove(&start.key));
        cx.notify();
        let Some(height) = height.filter(|h| *h != start.embed.height) else {
            return;
        };
        let markdown = docs::embed(&start.embed.doc, &start.title, &start.embed.view, height);
        self.state.update(cx, |state, cx| {
            if state.is_readonly() {
                return;
            }
            if let Some(block) = embed_block(state, &start) {
                let _ = state.run(EditorCommand::ReplaceBlock { block, markdown }, cx);
            }
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

    /// Keeps the table of contents' headings up to date.
    fn update_headings(&self, cx: &App) {
        let state = self.state.read(cx);
        let headings: Vec<Heading> = state
            .model()
            .document()
            .leaves()
            .filter_map(|b| match b.kind() {
                BlockKind::Heading { level } => Some(Heading {
                    level,
                    text: b.text().unwrap_or_default().replace('\u{FFFC}', "").into(),
                    block: b.id(),
                }),
                _ => None,
            })
            .collect();
        if let Ok(mut list) = self.headings.lock() {
            *list = headings;
        }
    }

    /// Puts the caret at the start of `block` and scrolls to it.
    fn go_to(&mut self, block: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            if state
                .set_selection(BlockSelection::caret(BlockPosition::new(block, 0)), cx)
                .is_ok()
            {
                state.reveal_caret(cx);
            }
        });
        self.focus(window, cx);
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
        if self.step_mention(by, cx) {
            return;
        }
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
        if let Some(selected) = self.mention.as_ref().map(|m| m.selected) {
            if !self.pick_mention(selected, window, cx) {
                cx.propagate();
            }
            return;
        }
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
        if self.dismiss_mention(cx) {
            return;
        }
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
            SlashAction::Embed => Some(EditorEvent::EmbedRequested),
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
                    SlashAction::NewPage | SlashAction::LinkToPage | SlashAction::Embed => {}
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

    /// Whether the editor is laid out for a phone: narrower than [`Options::compact_below`], by
    /// its width when it was last drawn (the window's before that).
    pub(crate) fn compact(&self, window: &Window) -> bool {
        let width = self.scroll.bounds().size.width;
        let width = if width > px(0.) {
            width
        } else {
            window.viewport_size().width
        };
        width < self.compact_below
    }

    /// Whether the side menu (the + and the grip) shows: not in the compact layout, which has
    /// no room for it, nor by touch, where nothing hovers; the long press's menu is there.
    fn shows_side_menu(&self, window: &Window) -> bool {
        !self.compact(window) && !window.last_input_was_touch()
    }

    /// The pointer moved: the side menu goes beside the top-level block level with it.
    fn hover(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button.is_some() || self.state.read(cx).is_readonly() {
            return;
        }
        if !self.shows_side_menu(window) {
            if self.hovered.take().is_some() {
                cx.notify();
            }
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
        // Beside the block whose menu is open, else the block under the pointer.
        let block = self.block_menu.or(self.hovered)?;
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
        let grip = button("blocks-side-grip", "⋮⋮")
            .cursor_grab()
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                cx.stop_propagation();
                // Shift: the blocks from the selected ones to this one are selected, whole.
                if event.modifiers.shift {
                    let _ = this_drag.update(cx, |this, cx| this.extend_block_selection(block, cx));
                    return;
                }
                // The editor follows the pointer from here and drops the block where it's let
                // go, as one undoable move; let go without moving, it opens the block menu.
                state.update(cx, |s, _| s.begin_block_drag(block, event.position));
                let _ = this_drag.update(cx, |this, cx| {
                    this.hovered = None;
                    this.grip_pressed(block);
                    cx.notify();
                });
            })
            .into_any_element();
        let grip = self.grip_with_menu(block, grip, cx);
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
                        .child(grip),
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
/// The block of the embed a resize started on: the one block written as its Markdown, or among
/// several the one under where the press was.
fn embed_block(state: &BlockEditorState, start: &ResizeStart) -> Option<BlockId> {
    fn walk(block: markdown_editor::BlockRef<'_>, markdown: &str, out: &mut Vec<BlockId>) {
        if block.kind() == BlockKind::Custom && block.markdown() == markdown {
            out.push(block.id());
        }
        for child in block.children() {
            walk(child, markdown, out);
        }
    }
    let doc = state.model().document();
    let mut found = Vec::new();
    walk(doc.root(), &start.markdown, &mut found);
    if found.len() <= 1 {
        return found.pop();
    }
    let top = state
        .top_level_blocks()
        .into_iter()
        .find(|(_, b)| b.contains(&start.at))?
        .0;
    found
        .into_iter()
        .find(|id| top_level(state, *id) == Some(top))
}

/// What a pasted document link can become.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PasteAs {
    Mention,
    Page,
    Embed,
}

/// A line holding only a pasted document link.
#[derive(Clone, Debug)]
struct PasteOffer {
    block: BlockId,
    doc: String,
    title: String,
    /// Pasted as the text `doc:id`, not yet a mention.
    plain: bool,
}

/// The offer for the caret's line, when it's a paragraph holding only a document link.
fn paste_offer(
    state: &Entity<BlockEditorState>,
    docs: Option<&dyn Docs>,
    cx: &App,
) -> Option<PasteOffer> {
    let state = state.read(cx);
    let model = state.model();
    let caret = model.caret()?;
    let block = model.document().block(caret.block)?;
    if block.kind() != BlockKind::Paragraph {
        return None;
    }
    let text = block.text()?.trim();
    // Text from another app: `[title](doc:id)` pasted as it was written.
    if let Some((doc, title)) = docs::mention_of(text) {
        return Some(PasteOffer {
            block: caret.block,
            doc,
            title,
            plain: true,
        });
    }
    if let Some(id) = docs::doc_id(text) {
        let title = docs
            .and_then(|d| d.info(id, cx))
            .map(|i| i.title.to_string())
            .unwrap_or_else(|| "Untitled".into());
        return Some(PasteOffer {
            block: caret.block,
            doc: id.to_string(),
            title,
            plain: true,
        });
    }
    if text != "\u{FFFC}" {
        return None;
    }
    let (doc, title) = docs::mention_of(&block.inline_markdown()?)?;
    Some(PasteOffer {
        block: caret.block,
        doc,
        title,
        plain: false,
    })
}

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

    /// On a touch screen, Cut, Copy, Paste, Select all and Block… (the block menu, in a sheet)
    /// above a text selection (Copy and Select all when read-only), as iOS shows them. Hidden
    /// while a handle moves, and after an item is used until the selection changes.
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
        // The format bar's marks and Link, as a second row.
        let format_row = (!readonly).then(|| self.format_buttons(px(44.), cx));
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
        if !readonly {
            let this = cx.entity().downgrade();
            items.push(
                div()
                    .id("Block…")
                    .h(px(44.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .text_size(px(15.))
                    .whitespace_nowrap()
                    .child("Block…")
                    .debug_selector(|| "blocks-menu-Block".into())
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let _ = this.update(cx, |this, cx| {
                            this.used_menu(cx);
                            this.open_block_sheet(cx);
                        });
                    }),
            );
        }
        Some(
            deferred(
                anchored()
                    .position(at)
                    .anchor(Anchor::BottomCenter)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(border)
                            .bg(bg)
                            .text_color(fg)
                            .shadow_md()
                            .overflow_hidden()
                            .debug_selector(|| "blocks-edit-menu".into())
                            .child(div().flex().children(items))
                            .when_some(format_row, |menu, row| {
                                menu.child(
                                    div()
                                        .flex()
                                        .justify_center()
                                        .border_t_1()
                                        .border_color(border)
                                        .children(row),
                                )
                            }),
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
        let compact = self.compact(window);
        let side_menu = if self.shows_side_menu(window) {
            self.side_menu(cx)
        } else {
            None
        };
        let (gutter, top) = if compact {
            (px(COMPACT_INSET), px(COMPACT_INSET))
        } else {
            (self.gutter, self.top)
        };
        let slash_menu = self.slash_menu(cx);
        let mention_menu = self.mention_menu(cx);
        let handles = self.selection_handles(window, cx);
        let paste_menu = self.paste_menu(cx);
        let resize = self.resizing.is_some().then(|| {
            // While an embed's height is dragged, the pointer is followed anywhere in the window.
            let this = cx.entity().downgrade();
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let moving = this.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ =
                                moving.update(cx, |this, cx| this.resize_to(event.position.y, cx));
                        }
                    });
                    let up = this.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = up.update(cx, |this, cx| this.finish_resize(cx));
                        }
                    });
                },
            )
            .size_0()
        });
        let edit_menu = self.edit_menu(window, cx);
        let format_bar = self.format_bar(window, cx);
        let block_sheet = self.block_sheet(cx);
        let find_bar = self.find_bar(compact, cx);
        // While a grip is pressed: its release, seen before the editor ends the drag.
        let grip_watch = self.grip_down.is_some().then(|| {
            let this = cx.entity().downgrade();
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let up = this.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = up.update(cx, |this, cx| this.grip_released(cx));
                        }
                    });
                },
            )
            .size_0()
        });
        let page = div()
            .id("gpui-blocks")
            .size_full()
            .key_context(if self.slash.is_some() || self.mention.is_some() {
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
                    .w(px(COLUMN) + gutter * 2.)
                    .max_w_full()
                    .mx_auto()
                    .min_h_full()
                    .child(
                        // Padding on the editor itself, so the whole column takes clicks, and the
                        // gutter on its left holds the side menu.
                        BlockEditor::new(&self.state)
                            .w_full()
                            .min_h_full()
                            .px(gutter)
                            .pt(top)
                            .pb(px(240.)),
                    ),
            )
            .children(side_menu)
            .children(slash_menu)
            .children(mention_menu)
            .children(handles)
            .children(paste_menu)
            .children(resize)
            .children(edit_menu)
            .children(format_bar)
            .child(block_sheet)
            .children(grip_watch)
            .child(self.long_press_watch(cx));
        // The find bar stays at the top while the page scrolls under it.
        div()
            .relative()
            .size_full()
            .on_action(
                cx.listener(|this, _: &FindOpen, window, cx| this.open_find(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FindReplaceOpen, window, cx| {
                    this.open_find(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &FindNext, _, cx| this.step_find(true, cx)))
            .on_action(cx.listener(|this, _: &FindPrevious, _, cx| this.step_find(false, cx)))
            .on_action(cx.listener(|this, _: &FindClose, window, cx| this.close_find(window, cx)))
            .child(page)
            .children(find_bar)
    }
}

#[cfg(test)]
mod tests {
    use super::{COMPACT_INSET, Editor, GUTTER, Options, TOP};
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use markdown_editor::{BlockKind, BlockPosition, BlockSelection};
    use std::prelude::rust_2024::test;

    fn open<'a>(
        cx: &'a mut TestAppContext,
        source: &str,
    ) -> (Entity<Editor>, &'a mut VisualTestContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let source = source.to_string();
        let (editor, cx) = cx.add_window_view(move |window, cx| Editor::new(&source, window, cx));
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        (editor, cx)
    }

    /// A comment's anchor: the selection's block by its lasting id and the selected text; going
    /// back to it by that id; and where its top-level block is drawn.
    #[gpui::test]
    fn comments_anchor_to_blocks_by_their_lasting_ids(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "intro\n\n- an **item** here\n\nlast");
        // Select "item here" in the list item's paragraph.
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let id = s.model().document().leaves().nth(1).unwrap().id();
                s.set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(id, 3),
                        head: BlockPosition::new(id, 12),
                    },
                    cx,
                )
                .unwrap();
            })
        });
        let (id, text) = editor.update(cx, |e, cx| e.selection_anchor(cx)).unwrap();
        assert_eq!(text, "item here");
        // The same id the document gives that paragraph.
        let doc = editor.update(cx, |e, cx| e.document(cx));
        let item_paragraph = &doc.blocks[1].children[0].children[0];
        assert_eq!(item_paragraph.id, id);
        // Going to the last paragraph by its id puts the caret at its start.
        let last = doc.blocks[2].id.clone();
        assert!(editor.update_in(cx, |e, window, cx| e.reveal_block(&last, window, cx)));
        let caret = editor.update(cx, |e, cx| e.state().read(cx).model().caret().unwrap());
        let last_leaf = editor.update(cx, |e, cx| {
            e.state()
                .read(cx)
                .model()
                .document()
                .leaves()
                .nth(2)
                .unwrap()
                .id()
        });
        assert_eq!((caret.block, caret.offset), (last_leaf, 0));
        assert!(!editor.update_in(cx, |e, window, cx| e.reveal_block("nope", window, cx)));
        // The list's bounds, from its item's paragraph.
        draw(cx);
        let bounds = editor.update(cx, |e, cx| e.block_bounds(&id, cx));
        assert!(bounds.is_some_and(|b| b.size.height > gpui::px(0.)));
        assert!(
            editor
                .update(cx, |e, cx| e.block_bounds("nope", cx))
                .is_none()
        );
    }

    /// Switching an embedded database's view rewrites its embeds that showed the old view, and
    /// only those, as one undo step.
    #[gpui::test]
    fn an_embeds_view_is_rewritten_in_place(cx: &mut TestAppContext) {
        let source = format!(
            "{}\n\n{}\n\n{}",
            crate::docs::embed("db1", "Tasks", "table", 300),
            crate::docs::embed("db1", "Tasks", "board", 300),
            crate::docs::embed("db2", "Other", "table", 200)
        );
        let (editor, cx) = open(cx, &source);
        let n = editor.update(cx, |e, cx| {
            e.set_embed_view("db1", "table", "view-42_B", cx)
        });
        assert_eq!(n, 1);
        let want = format!(
            "{}\n\n{}\n\n{}",
            crate::docs::embed("db1", "Tasks", "view-42_B", 300),
            crate::docs::embed("db1", "Tasks", "board", 300),
            crate::docs::embed("db2", "Other", "table", 200)
        );
        assert_eq!(editor.update(cx, |e, cx| e.markdown(cx)), want);
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-z"
        } else {
            "ctrl-z"
        });
        draw(cx);
        assert_eq!(
            editor.update(cx, |e, cx| e.markdown(cx)),
            source,
            "one undo step"
        );
    }

    /// Wide, the text starts the gutter in and a hover shows the side menu; at a phone's width
    /// the editor lays itself out compact: 16 in, 16 from the top, and no side menu.
    #[gpui::test]
    fn a_narrow_editor_lays_itself_out_for_a_phone(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "first\n\nsecond");
        // The text's inset from the editor's left edge and top, and whether a hover over the
        // first block shows the side menu.
        let layout = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            draw(cx);
            draw(cx);
            let (view, block) = editor.update(cx, |e, cx| {
                let view = e.scroll.bounds();
                let blocks = e.state().read(cx).top_level_blocks();
                (view, blocks[0].1)
            });
            cx.simulate_mouse_move(block.center(), None, gpui::Modifiers::none());
            draw(cx);
            let hovered = editor.read_with(cx, |e, _| e.hovered.is_some());
            (
                block.left() - view.left(),
                block.top() - view.top(),
                hovered,
            )
        };
        cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
        let (left, top, hovered) = layout(&editor, cx);
        assert!(left >= gpui::px(GUTTER), "wide: {left:?} in");
        assert!(top >= gpui::px(TOP), "wide: {top:?} down");
        assert!(hovered, "wide: the side menu shows");
        assert!(!editor.update_in(cx, |e, window, _| e.compact(window)));

        cx.simulate_resize(gpui::size(gpui::px(390.), gpui::px(800.)));
        let (left, top, hovered) = layout(&editor, cx);
        assert!(editor.update_in(cx, |e, window, _| e.compact(window)));
        assert_eq!(left, gpui::px(COMPACT_INSET), "compact: 16 in");
        assert!(top < gpui::px(COMPACT_INSET * 2.), "compact: {top:?} down");
        assert!(!hovered, "compact: no side menu");
    }

    /// `compact_below(px(0.))` keeps the wide layout on a narrow editor, with the host's own
    /// insets.
    #[gpui::test]
    fn a_host_can_keep_the_wide_layout_with_its_own_insets(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let options = Options::default()
                .gutter(gpui::px(24.))
                .top(gpui::px(8.))
                .compact_below(gpui::px(0.));
            Editor::with_options("first", options, window, cx)
        });
        cx.simulate_resize(gpui::size(gpui::px(390.), gpui::px(800.)));
        draw(cx);
        draw(cx);
        let (view, block) = editor.update(cx, |e, cx| {
            (
                e.scroll.bounds(),
                e.state().read(cx).top_level_blocks()[0].1,
            )
        });
        assert!(!editor.update_in(cx, |e, window, _| e.compact(window)));
        assert_eq!(block.left() - view.left(), gpui::px(24.));
    }

    /// ⌘F opens the find bar with the selected word, Enter steps on, Esc closes it and stops
    /// the highlighting; ⌘⌥F's replace row replaces with Enter, keeping undo whole.
    #[gpui::test]
    fn the_find_bar_finds_steps_replaces_and_closes(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "plan one\n\nplan two\n\nplan three");
        // Select the first "plan".
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let id = s.model().document().leaves().next().unwrap().id();
                s.set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(id, 0),
                        head: BlockPosition::new(id, 4),
                    },
                    cx,
                )
                .unwrap();
            })
        });
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-f"
        } else {
            "ctrl-f"
        });
        draw(cx);
        let found = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| {
                let (matches, current) = e.state().read(cx).find_matches();
                (matches.len(), current)
            })
        };
        assert!(
            editor.read_with(cx, |e, _| e.find.is_some()),
            "the bar opened"
        );
        assert_eq!(
            found(&editor, cx),
            (3, Some(0)),
            "the selected word is the query"
        );
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(found(&editor, cx).1, Some(1));
        cx.simulate_keystrokes("shift-enter");
        draw(cx);
        assert_eq!(found(&editor, cx).1, Some(0));
        // The replace row: type a replacement, Enter replaces the current match.
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-alt-f"
        } else {
            "ctrl-h"
        });
        draw(cx);
        editor.update_in(cx, |e, window, cx| {
            let replace = e.find.as_ref().unwrap().replace.clone();
            replace.update(cx, |r, cx| {
                r.set_value("goal", window, cx);
                r.focus(window, cx);
            });
        });
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(
            editor.update(cx, |e, cx| e.markdown(cx)),
            "goal one\n\nplan two\n\nplan three"
        );
        assert_eq!(found(&editor, cx), (2, Some(0)), "on to the next");
        // Esc closes the bar and the highlighting goes.
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert!(editor.read_with(cx, |e, _| e.find.is_none()));
        assert_eq!(found(&editor, cx), (0, None));
    }

    /// A press on a grip that doesn't drag opens the block menu; its entries turn the block into
    /// a heading, duplicate it and delete it.
    #[gpui::test]
    fn the_block_menu_turns_duplicates_and_deletes(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "first\n\nsecond");
        let block = |editor: &Entity<Editor>, cx: &mut VisualTestContext, ix: usize| {
            editor.update(cx, |e, cx| {
                e.state()
                    .read(cx)
                    .model()
                    .document()
                    .root()
                    .children()
                    .nth(ix)
                    .unwrap()
                    .id()
            })
        };
        let first = block(&editor, cx, 0);
        editor.update(cx, |e, cx| {
            e.grip_pressed(first);
            e.grip_released(cx);
        });
        assert_eq!(editor.read_with(cx, |e, _| e.block_menu), Some(first));
        draw(cx);
        // Turn into: Heading 1 (the slash menu's second built-in block).
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(first, "turn-1", window, cx)
        });
        assert_eq!(
            editor.update(cx, |e, cx| e.markdown(cx)),
            "# first\n\nsecond"
        );
        assert_eq!(
            editor.read_with(cx, |e, _| e.block_menu),
            None,
            "picking closes it"
        );
        let first = block(&editor, cx, 0);
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(first, "duplicate", window, cx)
        });
        assert_eq!(
            editor.update(cx, |e, cx| e.markdown(cx)),
            "# first\n\n# first\n\nsecond"
        );
        let copy = block(&editor, cx, 1);
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(copy, "delete", window, cx)
        });
        assert_eq!(
            editor.update(cx, |e, cx| e.markdown(cx)),
            "# first\n\nsecond"
        );
    }

    /// Shift-clicks on grips select the blocks between, whole; a first one selects its block.
    #[gpui::test]
    fn shift_clicks_on_grips_select_blocks(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "a\n\nb\n\nc");
        let ids: Vec<markdown_editor::BlockId> = editor.update(cx, |e, cx| {
            e.state()
                .read(cx)
                .model()
                .document()
                .root()
                .children()
                .map(|b| b.id())
                .collect()
        });
        let selection = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| e.state().read(cx).model().selection().clone())
        };
        editor.update(cx, |e, cx| e.extend_block_selection(ids[0], cx));
        assert_eq!(
            selection(&editor, cx),
            BlockSelection::Blocks {
                anchor: ids[0],
                head: ids[0]
            }
        );
        editor.update(cx, |e, cx| e.extend_block_selection(ids[2], cx));
        assert_eq!(
            selection(&editor, cx),
            BlockSelection::Blocks {
                anchor: ids[0],
                head: ids[2]
            },
            "from the first to the third"
        );
        // A selection of the first two moves down past the third as one run.
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                s.set_selection(
                    BlockSelection::Blocks {
                        anchor: ids[0],
                        head: ids[0],
                    },
                    cx,
                )
                .unwrap();
            });
            e.extend_block_selection(ids[1], cx);
            e.move_block(true, cx);
        });
        draw(cx);
        assert_eq!(editor.update(cx, |e, cx| e.markdown(cx)), "c\n\na\n\nb");
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

    #[gpui::test]
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

    #[gpui::test]
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

    #[gpui::test]
    fn a_slash_inside_a_word_or_with_no_match_doesnt_open(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("and/or");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
        cx.simulate_input(" /zzz");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
    }

    #[gpui::test]
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

    #[gpui::test]
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

    #[gpui::test]
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
    ) -> gpui::Point<gpui::Pixels> {
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

    fn touch(cx: &mut VisualTestContext, phase: gpui::TouchPhase, at: gpui::Point<gpui::Pixels>) {
        cx.simulate_event(gpui::TouchEvent {
            id: gpui::TouchId(7),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        draw(cx);
    }

    /// Drags the selection handle `name` by touch to `to`, from its knob.
    fn drag_handle(cx: &mut VisualTestContext, name: &'static str, to: gpui::Point<gpui::Pixels>) {
        use gpui::TouchPhase::*;
        let handle = cx.debug_bounds(name).expect("the handle is drawn");
        let knob = if name.ends_with("start") {
            gpui::point(handle.center().x, handle.top() + gpui::px(5.))
        } else {
            gpui::point(handle.center().x, handle.bottom() - gpui::px(5.))
        };
        let offset = knob - handle.center();
        touch(cx, Started, knob);
        let halfway = gpui::point((knob.x + to.x + offset.x) / 2., knob.y);
        touch(cx, Moved, halfway);
        touch(cx, Moved, to + offset);
        touch(cx, Ended, to + offset);
    }

    #[gpui::test]
    fn touch_handles_move_the_ends_of_a_selection(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world again");
        let word = point_at(&editor, cx, 8);
        // A finger held on a word: GPUI recognizes the long press after a moment.
        touch(cx, gpui::TouchPhase::Started, word);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        draw(cx);
        touch(cx, gpui::TouchPhase::Ended, word);
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

    fn long_press(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
        touch(cx, gpui::TouchPhase::Started, at);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        draw(cx);
        touch(cx, gpui::TouchPhase::Ended, at);
    }

    fn tap(cx: &mut VisualTestContext, name: &'static str) {
        let at = cx
            .debug_bounds(name)
            .unwrap_or_else(|| panic!("{name} isn't drawn"))
            .center();
        touch(cx, gpui::TouchPhase::Started, at);
        touch(cx, gpui::TouchPhase::Ended, at);
    }

    #[gpui::test]
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
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("planet".into()));
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        tap(cx, "blocks-menu-Paste");
        assert_eq!(markdown(&editor, cx), "hello planet again");
        let at = point_at(&editor, cx, 2);
        long_press(cx, at);
        tap(cx, "blocks-menu-Select all");
        assert_eq!(selected(&editor, cx), (0, 18));
    }

    /// On touch, where there's no grip, the edit menu's Block… opens the block menu in a sheet
    /// for the top-level block the selection is in; its entries act on that block.
    #[gpui::test]
    fn the_touch_menu_reaches_the_block_menu_in_a_sheet(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "intro words\n\nsecond line");
        let top = |editor: &Entity<Editor>, cx: &mut VisualTestContext, ix: usize| {
            editor.update(cx, |e, cx| {
                e.state()
                    .read(cx)
                    .model()
                    .document()
                    .root()
                    .children()
                    .nth(ix)
                    .map(|b| b.id())
            })
        };
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        tap(cx, "blocks-menu-Block");
        draw(cx);
        assert_eq!(
            editor.read_with(cx, |e, _| e.block_sheet),
            top(&editor, cx, 0),
            "the sheet is for the first paragraph"
        );
        assert!(cx.debug_bounds("gpui-core-sheet").is_some());
        assert!(
            cx.debug_bounds("blocks-edit-menu").is_none(),
            "the edit menu closes"
        );
        // Turn into: Heading 1.
        let heading = cx
            .debug_bounds("menu-item-turn-1")
            .expect("Turn into's kinds are in the sheet")
            .center();
        cx.simulate_click(heading, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(markdown(&editor, cx), "# intro words\n\nsecond line");
        assert_eq!(editor.read_with(cx, |e, _| e.block_sheet), None);
        assert!(
            cx.debug_bounds("gpui-core-sheet").is_none(),
            "picking closes it"
        );
        // Delete, from a long press in the second paragraph.
        let second = editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            let leaf = state.model().document().leaves().nth(1).unwrap().id();
            state
                .position_bounds(BlockPosition::new(leaf, 2))
                .unwrap()
                .center()
        });
        long_press(cx, second);
        tap(cx, "blocks-menu-Block");
        draw(cx);
        assert_eq!(
            editor.read_with(cx, |e, _| e.block_sheet),
            top(&editor, cx, 1)
        );
        let delete = cx.debug_bounds("menu-item-delete").unwrap().center();
        cx.simulate_click(delete, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(markdown(&editor, cx), "# intro words");
    }

    /// Drags out a selection with the pointer from `from` to `to` (byte offsets in the first
    /// line), checking the bar waits for the button to come up.
    fn drag_select(editor: &Entity<Editor>, cx: &mut VisualTestContext, from: usize, to: usize) {
        let (a, b) = (point_at(editor, cx, from), point_at(editor, cx, to));
        cx.simulate_mouse_down(a, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
        cx.simulate_mouse_move(b, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
        draw(cx);
        assert!(
            cx.debug_bounds("blocks-format-bar").is_none(),
            "no bar while the selection is being dragged out"
        );
        cx.simulate_mouse_up(b, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
    }

    fn press(cx: &mut VisualTestContext, name: &'static str) {
        let at = cx
            .debug_bounds(name)
            .unwrap_or_else(|| panic!("{name} isn't drawn"))
            .center();
        cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
    }

    /// A selection made with the pointer shows the format bar once the button is up; its
    /// buttons toggle marks over the selection and show which are on.
    #[gpui::test]
    fn the_format_bar_toggles_marks_on_a_pointer_selection(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world again");
        assert!(cx.debug_bounds("blocks-format-bar").is_none());
        drag_select(&editor, cx, 6, 11);
        assert!(
            cx.debug_bounds("blocks-format-bar").is_some(),
            "the bar shows"
        );
        press(cx, "blocks-format-Bold");
        assert_eq!(markdown(&editor, cx), "hello **world** again");
        let bold = editor.update(cx, |e, cx| {
            e.state()
                .read(cx)
                .model()
                .mark_state(markdown_editor::MarkKind::Bold)
        });
        assert_eq!(bold, markdown_editor::MarkState::On, "the selection stays");
        press(cx, "blocks-format-Italic");
        press(cx, "blocks-format-Bold");
        assert_eq!(markdown(&editor, cx), "hello *world* again");
        press(cx, "blocks-format-Code");
        assert!(markdown(&editor, cx).contains('`'));
        // A click elsewhere collapses the selection, and the bar goes.
        let start = point_at(&editor, cx, 1);
        cx.simulate_click(start, gpui::Modifiers::none());
        draw(cx);
        assert!(cx.debug_bounds("blocks-format-bar").is_none());
    }

    /// Link asks the app for the link, as ⌘K does; Turn into turns the selection's block.
    #[gpui::test]
    fn the_format_bar_asks_for_links_and_turns_blocks(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world");
        let asked = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen = asked.clone();
        cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &super::EditorEvent, _| {
                if let super::EditorEvent::EditLinkRequested { .. } = event {
                    seen.set(true);
                }
            })
            .detach()
        });
        drag_select(&editor, cx, 0, 5);
        press(cx, "blocks-format-Link");
        assert!(asked.get(), "the app is asked for the link");
        press(cx, "blocks-format-turn");
        let heading = cx
            .debug_bounds("menu-item-turn-1")
            .expect("Turn into's menu is open")
            .center();
        cx.simulate_click(heading, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(markdown(&editor, cx), "# hello world");
    }

    /// No bar in a read-only document; on touch the marks are the edit menu's second row.
    #[gpui::test]
    fn touch_formats_from_the_edit_menu_and_read_only_has_no_bar(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world again");
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        assert!(
            cx.debug_bounds("blocks-format-bar").is_none(),
            "no bar on touch"
        );
        tap(cx, "blocks-format-Bold");
        assert_eq!(markdown(&editor, cx), "hello **world** again");
        assert!(
            cx.debug_bounds("blocks-edit-menu").is_some(),
            "the menu stays for another mark"
        );
        editor.update(cx, |e, cx| e.set_readonly(true, cx));
        draw(cx);
        drag_select(&editor, cx, 0, 5);
        assert!(cx.debug_bounds("blocks-format-bar").is_none());
    }

    #[gpui::test]
    fn a_read_only_touch_menu_only_copies(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world");
        editor.update(cx, |e, cx| e.set_readonly(true, cx));
        draw(cx);
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        assert!(cx.debug_bounds("blocks-menu-Copy").is_some());
        assert!(cx.debug_bounds("blocks-menu-Cut").is_none());
        assert!(cx.debug_bounds("blocks-menu-Paste").is_none());
        assert!(cx.debug_bounds("blocks-menu-Block").is_none());
    }

    #[gpui::test]
    fn a_selection_made_with_a_mouse_has_no_handles(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world");
        let word = point_at(&editor, cx, 8);
        cx.simulate_click(word, gpui::Modifiers::none());
        cx.simulate_event(gpui::MouseDownEvent {
            button: gpui::MouseButton::Left,
            position: word,
            modifiers: gpui::Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            button: gpui::MouseButton::Left,
            position: word,
            modifiers: gpui::Modifiers::none(),
            click_count: 2,
        });
        draw(cx);
        assert_eq!(selected(&editor, cx), (6, 11));
        assert!(cx.debug_bounds("blocks-handle-start").is_none());
    }

    #[gpui::test]
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
            gpui::ImageSource::Image(Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Svg,
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

    #[gpui::test]
    fn the_table_of_contents_follows_the_headings(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "[TOC]\n\n# One\n\n## Two\n\nText");
        assert!(cx.debug_bounds("blocks-toc").is_some());
        assert!(cx.debug_bounds("blocks-toc-1").is_some());
        assert!(cx.debug_bounds("blocks-toc-2").is_none());
        // A heading typed at the end is listed too.
        let last = editor.update(cx, |e, cx| {
            e.state().read(cx).model().document().leaves().count() - 1
        });
        caret_at_end(&editor, cx, last);
        cx.simulate_keystrokes("enter");
        cx.simulate_input("## Three");
        draw(cx);
        assert!(
            cx.debug_bounds("blocks-toc-2").is_some(),
            "{}",
            markdown(&editor, cx)
        );
        // A click on Two puts the caret at its start.
        let two = cx.debug_bounds("blocks-toc-1").unwrap().center();
        cx.simulate_click(two, gpui::Modifiers::none());
        draw(cx);
        let at = editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            let caret = state.model().caret().unwrap();
            (
                state
                    .model()
                    .document()
                    .block(caret.block)
                    .and_then(|b| b.text())
                    .map(str::to_string),
                caret.offset,
            )
        });
        assert_eq!(at, (Some("Two".to_string()), 0));
        assert_eq!(
            markdown(&editor, cx),
            "[TOC]\n\n# One\n\n## Two\n\nText\n\n## Three"
        );
    }

    #[gpui::test]
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

    #[gpui::test]
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

    #[gpui::test]
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

    #[gpui::test]
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

    #[gpui::test]
    fn picking_a_lists_own_kind_keeps_the_item(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "- item");
        caret_at_end(&editor, cx, 0);
        cx.simulate_input(" /bullet");
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "- item ");
    }

    #[gpui::test]
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

    #[gpui::test]
    fn dividers_and_widgets_get_a_side_menu_too(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "a\n\n---\n\nb");
        let blocks = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks().len());
        assert_eq!(blocks, 3);
    }

    #[gpui::test]
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

    #[gpui::test]
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

    #[gpui::test]
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
