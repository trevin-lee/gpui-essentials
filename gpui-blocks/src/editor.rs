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
use markdown_editor::Control as _;
use markdown_editor::{
    BlockEditor, BlockEditorEvent, BlockEditorState, BlockId, BlockKind, BlockPosition,
    BlockSelection, EditOrigin, EditorCommand, Extensions,
};

use std::path::Path;
use std::sync::Arc;

use crate::actions::*;
use crate::bookmark::{Bookmark, BookmarkHooks, BookmarkPlugin, Unfurl, UnfurlFn};
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
/// The compact layout's room each side of the column: wide enough, by touch, for a long press
/// left of the text to lift its block.
const COMPACT_SIDE: f32 = 24.;

/// The room below the last line, so it can be scrolled up from the window's bottom edge.
const PAGE_BOTTOM: f32 = 240.;

/// How far below the top of the page a block gone to lands.
const GO_TO_MARGIN: f32 = 48.;

/// The frames a block gone to is waited for.
const GO_TO_TRIES: u8 = 6;

/// Where a place gone to lands on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// Near the top: a heading, with its section below it.
    Top,
    /// In the middle: a found match, with the lines around it.
    Middle,
}

/// How much of the page Page Down keeps in view from before: a few lines.
const PAGE_OVERLAP: f32 = 64.;

/// Binds the slash menu's keys and the keys that move blocks. They match only inside this view
/// (key context `BlocksEditor`, or `BlocksSlashMenu` while the menu is open), and are bound
/// after the editor's own, so ↑, ↓, Enter, Tab and Esc go to the menu while it's open. A
/// focused handle's keys match in its own context. An app can bind the actions in
/// [`crate::actions`] to other keys after [`crate::init`].
pub(crate) fn bind_keys(cx: &mut App) {
    // A focused handle's keys, as ARIA's separators take them.
    for (context, [less, more, less_fine, more_fine]) in [
        (
            crate::handles::COLUMN_HANDLE,
            ["left", "right", "shift-left", "shift-right"],
        ),
        (
            crate::handles::EMBED_HANDLE,
            ["up", "down", "shift-up", "shift-down"],
        ),
    ] {
        let at = Some(context);
        cx.bind_keys([
            KeyBinding::new(less, HandleLess, at),
            KeyBinding::new(more, HandleMore, at),
            KeyBinding::new(less_fine, HandleLessFine, at),
            KeyBinding::new(more_fine, HandleMoreFine, at),
            KeyBinding::new("home", HandleLeast, at),
            KeyBinding::new("end", HandleMost, at),
            KeyBinding::new("escape", HandleLeave, at),
            KeyBinding::new("enter", HandleLeave, at),
        ]);
    }
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
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-h"
            } else {
                "ctrl-shift-h"
            },
            HighlightAgain,
            editor,
        ),
        // Page Up and Down (fn-↑ and fn-↓ on a Mac keyboard) move the caret a page, as
        // Notion's and the code editors' do, so typing doesn't jump back.
        KeyBinding::new("pagedown", PageDown, editor),
        KeyBinding::new("pageup", PageUp, editor),
        KeyBinding::new("shift-pagedown", SelectPageDown, editor),
        KeyBinding::new("shift-pageup", SelectPageUp, editor),
        // What a click does, by keyboard (Notion's keys).
        KeyBinding::new(if mac { "cmd-/" } else { "ctrl-/" }, OpenBlockMenu, editor),
        KeyBinding::new(
            if mac { "cmd-enter" } else { "ctrl-enter" },
            CheckOrFold,
            editor,
        ),
        KeyBinding::new("alt-enter", OpenAtCaret, editor),
        // A host app binds ⌘⇧M to its comment field too: the editor's handler answers it and
        // the key goes no further, so the field opens once.
        KeyBinding::new(
            if mac { "cmd-shift-m" } else { "ctrl-shift-m" },
            Comment,
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
        BookmarkHooks::default(),
    )
}

fn extensions_with(
    widgets: &[Arc<dyn Widget>],
    hooks: DocHooks,
    toc: TocHooks,
    embeds: EmbedHooks,
    chips: MentionHooks,
    bookmarks: BookmarkHooks,
) -> Extensions {
    let ext = Extensions::default()
        .with(TocPlugin(toc))
        .with(EmbedPlugin(embeds))
        .with(BookmarkPlugin(bookmarks))
        .with(PagePlugin(hooks.clone()))
        .with(MentionPlugin(hooks))
        .with(ChipPlugin(chips));
    widgets
        .iter()
        .fold(ext, |ext, widget| ext.with(WidgetPlugin(widget.clone())))
}

/// Reads the system clipboard's HTML (see [`Options::html_clipboard`]).
type HtmlClipboard = Arc<dyn Fn(&App) -> Option<String>>;

/// The most HTML a paste converts; a larger clipboard pastes as its plain text.
const MAX_PASTED_HTML: usize = 4 << 20;

/// Turns a picture picked from the slash menu's Image item into the Markdown destination to
/// write, or `None` to insert nothing.
type ImageLink = Arc<dyn Fn(&Path) -> Option<String>>;

/// Makes the link to a block from its lasting id (see [`Options::block_link`]).
type BlockLink = Arc<dyn Fn(&str) -> String>;

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
    html_clipboard: Option<HtmlClipboard>,
    comments: bool,
    block_link: Option<BlockLink>,
    unfurl: Option<UnfurlFn>,
    accessibility_label: SharedString,
    typography: bool,
    gutter: Pixels,
    top: Pixels,
    compact_below: Pixels,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            items: slash::default_items()
                .into_iter()
                .chain(slash::color_items())
                .collect(),
            image_link: None,
            images: None,
            highlighter: None,
            docs: None,
            embeds: None,
            mentions: None,
            html_clipboard: None,
            typography: true,
            comments: false,
            block_link: None,
            unfurl: None,
            accessibility_label: "Page".into(),
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
            .field("html_clipboard", &self.html_clipboard.is_some())
            .field("typography", &self.typography)
            .field("comments", &self.comments)
            .field("block_link", &self.block_link.is_some())
            .field("unfurl", &self.unfurl.is_some())
            .field("accessibility_label", &self.accessibility_label)
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

    /// Reads the HTML the system clipboard holds, if any, so a paste from a web page or Google
    /// Docs keeps its headings, lists, code, tables, links and marks (see [`crate::html`]).
    /// GPUI's clipboard has only the plain text, so the app reads the HTML itself: on macOS the
    /// general pasteboard's `public.html`, on iOS `UIPasteboard`'s `public.html`. Without it, or
    /// when it returns `None`, a paste is the plain text, as before. ⇧⌘V always pastes plain text.
    pub fn html_clipboard(mut self, read: impl Fn(&App) -> Option<String> + 'static) -> Self {
        self.html_clipboard = Some(Arc::new(read));
        self
    }

    /// The app takes comments on the text: the format bar gets Comment, as Notion's, which
    /// sends [`EditorEvent::CommentRequested`] with the selection's anchor.
    pub fn comments(mut self) -> Self {
        self.comments = true;
        self
    }

    /// The link to a block, for the block menu's Copy link to block, as Notion's: `link` gets
    /// the block's lasting id (as [`Editor::document`] and [`docs::Embed::block`] give it) and
    /// returns the link to copy, an address of this document with the block in it, say.
    /// Opening one is the app's: it opens the document and calls [`Editor::reveal_block`] with
    /// the id. Without it the menu has no Copy link to block.
    pub fn block_link(mut self, link: impl Fn(&str) -> String + 'static) -> Self {
        self.block_link = Some(Arc::new(link));
        self
    }

    /// How a new bookmark learns about its page (see [`crate::bookmark`]): `unfurl` gets the
    /// page's address and asks the app's server for its title, description and picture (the
    /// server's stored copy, an `asset:<sha256>`, which [`Options::images`] loads). The card
    /// shows the address until the answer comes, and stays so for `None`. The editor never
    /// fetches a page itself. Adds Bookmark to the slash menu, and Bookmark to the offer under
    /// a web address pasted on a line of its own.
    pub fn unfurl(
        mut self,
        unfurl: impl Fn(&str, &mut App) -> Task<Option<Unfurl>> + 'static,
    ) -> Self {
        self.unfurl = Some(Arc::new(unfurl));
        self.items.extend(slash::bookmark_items());
        self
    }

    /// What assistive technology calls the page's text, "Page" unless set: the document's title,
    /// say.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = label.into();
        self
    }

    /// Typing shortcuts, as Notion's, on unless turned off here: `->` becomes →, `<-` becomes ←,
    /// and `--` then a space becomes an em dash. Never in code, inline code or a web address,
    /// and one undo right after puts back what was typed.
    pub fn typing_shortcuts(mut self, on: bool) -> Self {
        self.typography = on;
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

    /// Below this width (600 by default) the editor lays itself out for a phone: 24 each side
    /// of the column and 16 above it, in place of [`Options::gutter`] and [`Options::top`], and no
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
    /// The slash menu's Bookmark: the app asks for the page's address, then calls
    /// [`Editor::insert_bookmark`].
    BookmarkRequested,
    /// A click on a person, agent or date mention (see [`crate::mentions`]): the app shows it.
    /// A page's mention sends [`EditorEvent::DocClicked`].
    MentionClicked { kind: MentionKind, id: SharedString },
    /// The format bar's Comment (see [`Options::comments`]): the app starts a thread on the
    /// selection, anchored as [`Editor::selection_anchor`] gives it: `block` is the lasting id
    /// of the block the selection starts in, and `quote` the selected words.
    CommentRequested { block: String, quote: String },
}

/// Gives the caret's line one of Notion's colours (its text's or its background), or takes
/// that colour off, keeping the caret where it is.
fn paint_line(
    state: &mut markdown_editor::BlockEditorState,
    text: bool,
    index: Option<usize>,
    cx: &mut Context<markdown_editor::BlockEditorState>,
) {
    use markdown_editor::{MarkChange, MarkKind};
    let Some(caret) = state.model().caret() else {
        return;
    };
    let Some(len) = state
        .model()
        .document()
        .block(caret.block)
        .and_then(|b| b.text())
        .map(str::len)
    else {
        return;
    };
    let change = match (text, index) {
        (true, Some(ix)) => markdown_editor::TEXT_COLORS
            .get(ix)
            .map(|c| MarkChange::new().with_color(c.color())),
        (false, Some(ix)) => markdown_editor::BACKGROUNDS
            .get(ix)
            .map(|c| MarkChange::new().with_highlight(c.color())),
        (true, None) => Some(MarkChange::new().clear(MarkKind::Color)),
        (false, None) => Some(MarkChange::new().clear(MarkKind::Highlight)),
    };
    let Some(change) = change.filter(|_| len > 0) else {
        return;
    };
    let line = BlockSelection::Text {
        anchor: BlockPosition::new(caret.block, 0),
        head: BlockPosition::new(caret.block, len),
    };
    if state.set_selection(line, cx).is_ok() {
        let _ = state.run(EditorCommand::ApplyMark(change), cx);
        let _ = state.set_selection(BlockSelection::caret(caret), cx);
    }
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

/// How near the page's top or bottom edge, in points, dragged blocks scroll it.
const EDGE_SCROLL_REACH: f32 = 56.;
/// The most the page scrolls a frame then, at the very edge.
const EDGE_SCROLL_SPEED: f32 = 14.;

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
    /// Whether the format bar's Color menu is open.
    pub(crate) format_color: bool,
    /// A columns block's width handle being dragged (see `columns_ui.rs`).
    pub(crate) column_drag: Option<crate::columns_ui::ColumnDrag>,
    /// The columns block whose width handles have the keyboard (see `handles.rs`).
    pub(crate) column_keys: Option<BlockId>,
    /// The width handles' focus, by columns block and place.
    pub(crate) column_focus:
        std::cell::RefCell<std::collections::HashMap<(BlockId, usize), FocusHandle>>,
    /// The embeds' height handles' focus, by the embed's lasting id.
    pub(crate) embed_focus: docs::HandleFocus,
    /// Steps a handle's keys made since one went down: one undo step.
    pub(crate) key_run: Option<crate::handles::KeyRun>,
    /// Whether the touch edit menu's Color sheet is open.
    pub(crate) color_sheet: bool,
    /// The colour last picked in the Color menu, which ⌘⇧H gives again.
    pub(crate) last_paint: Option<crate::format_bar::Paint>,
    /// The code block whose language menu is open (see `code_chrome.rs`).
    pub(crate) code_menu: Option<BlockId>,
    pub(crate) scroll: ScrollHandle,
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
    /// Reads the system clipboard's HTML, for pastes that keep their structure.
    html_clipboard: Option<HtmlClipboard>,
    /// Whether the format bar has Comment ([`Options::comments`]).
    pub(crate) comments: bool,
    /// Makes the block menu's Copy link to block's link.
    pub(crate) block_link: Option<BlockLink>,
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
    pub(crate) hovered: Option<BlockId>,
    /// The block under the pointer inside a hovered columns block, whose grip shows in the gap
    /// left of its column.
    hovered_inner: Option<BlockId>,
    /// Embeds' heights while one is being resized.
    embed_heights: docs::Heights,
    /// The embed being resized.
    resizing: Option<ResizeStart>,
    /// A document link pasted on a line of its own, which can become a sub-page or an embed,
    /// or a web address, which can become a bookmark.
    paste_offer: Option<PasteOffer>,
    /// Asks the app about a new bookmark's page.
    unfurl: Option<UnfurlFn>,
    /// The app's documents, for the title of a pasted `doc:` address.
    docs: Option<Arc<dyn Docs>>,
    /// The headings a table of contents lists, kept up to date after each change.
    headings: Arc<std::sync::Mutex<Vec<Heading>>>,
    /// The blocks' ids in [`Editor::document`].
    /// The blocks' lasting ids, shared with the embeds, which tell the app theirs.
    pub(crate) ids: Arc<std::sync::Mutex<crate::json::Ids>>,
    /// The comment anchors whose words are tinted (see [`Editor::highlight_quotes`]).
    pub(crate) quotes: Vec<(String, String)>,
    /// A selection handle being dragged by touch: the selection's other end, which stays, and
    /// how far the finger is from the end it moves.
    touch_drag: Option<(BlockPosition, Point<Pixels>)>,
    /// The selection the touch edit menu was used on: it stays hidden until the selection
    /// changes.
    menu_used_on: Option<(BlockPosition, BlockPosition)>,
    /// Page Up or Down waiting for the page it scrolled to be drawn: where in the window the
    /// caret goes, the selection's other end when it extends, and the frames left to wait.
    page_to: Option<(Point<Pixels>, Option<BlockPosition>, u8)>,
    /// A block gone to (from a table of contents or a link to it), to bring near the top of the
    /// page once it's drawn, and the frames left to wait for that.
    go_to_top: Option<(BlockPosition, u8, Place)>,
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
        let typography = options.typography;
        let accessibility_label = options.accessibility_label.clone();
        let highlighter = options.highlighter.clone();
        let embed_heights: docs::Heights = Arc::default();
        let embed_focus: docs::HandleFocus = Arc::default();
        let this = cx.entity().downgrade();
        let that = this.clone();
        let keyed = this.clone();
        let ids: Arc<std::sync::Mutex<crate::json::Ids>> = Arc::default();
        let embed_ids = ids.clone();
        let embeds = EmbedHooks {
            lasting_id: Some(Arc::new(move |block| {
                embed_ids
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .id_of(block)
            })),
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
            on_key: Some(Arc::new(
                move |start, key, window: &mut Window, cx: &mut App| {
                    let _ = keyed.update(cx, |this, cx| this.embed_key(start, key, window, cx));
                },
            )),
            focus: embed_focus.clone(),
        };
        let headings: Arc<std::sync::Mutex<Vec<Heading>>> = Arc::default();
        let this = cx.entity().downgrade();
        let bookmarks = BookmarkHooks {
            images: options.images.clone(),
            on_click: Some(Arc::new(move |url: &str, _: &mut Window, cx: &mut App| {
                let url = SharedString::from(url.to_string());
                let _ = this.update(cx, |_, cx| cx.emit(EditorEvent::LinkClicked { url }));
            })),
        };
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
                .with_markdown_extensions(extensions_with(
                    &widgets, hooks, toc, embeds, chips, bookmarks,
                ))
                .with_input_rules(
                    markdown_editor::InputRule::markdown_defaults()
                        .into_iter()
                        .chain(typography.then(markdown_editor::InputRule::typography)),
                )
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
            state.set_accessibility_label(accessibility_label, cx);
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
                    if !this.quotes.is_empty() {
                        this.refresh_tints(cx);
                    }
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
            html_clipboard: options.html_clipboard.clone(),
            comments: options.comments,
            block_link: options.block_link.clone(),
            unfurl: options.unfurl.clone(),
            mention: None,
            mention_dismissed: None,
            dismissed: None,
            open_on_next_change: false,
            hovered: None,
            hovered_inner: None,
            ids,
            quotes: Vec::new(),
            headings,
            embed_heights,
            embed_focus,
            column_keys: None,
            column_focus: Default::default(),
            key_run: None,
            resizing: None,
            paste_offer: None,
            docs: options.docs.clone(),
            touch_drag: None,
            find: None,
            grip_down: None,
            block_menu: None,
            block_sheet: None,
            format_turn: false,
            format_color: false,
            column_drag: None,
            color_sheet: false,
            last_paint: None,
            code_menu: None,
            menu_used_on: None,
            page_to: None,
            go_to_top: None,
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
        self.lock_ids().clear();
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
        crate::json::Document::from_model(
            state.model(),
            self.front_matter.clone(),
            &mut self.lock_ids(),
        )
    }

    /// Replaces the document with `document`, keeping its blocks' ids. Undo history starts over,
    /// and no [`EditorEvent::Changed`] is sent.
    pub fn set_document(&mut self, document: &crate::json::Document, cx: &mut Context<Self>) {
        self.set_markdown(&document.to_markdown(), cx);
        let state = self.state.read(cx);
        self.lock_ids().adopt(state.model(), &document.blocks);
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
        Some((self.lock_ids().id_of(block), lines.join("\n")))
    }

    /// Where the top-level block holding the block `id` (a lasting id, as in
    /// [`Editor::document`]) was last drawn, in window coordinates: for a mark in the margin
    /// beside it. `None` for an id the document doesn't have, or a block scrolled out of view.
    pub fn block_bounds(&self, id: &str, cx: &App) -> Option<Bounds<Pixels>> {
        let state = self.state.read(cx);
        let mut block = state
            .model()
            .document()
            .block(self.lock_ids().block_of(state.model(), id)?)?;
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
        let Some(block) = self.lock_ids().block_of(state.read(cx).model(), id) else {
            return false;
        };
        let leaf = state.update(cx, |state, cx| {
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
            let leaf = doc
                .leaves()
                .find(|leaf| inside(*leaf))
                .map(|leaf| leaf.id())?;
            let moved = state
                .set_selection(BlockSelection::caret(BlockPosition::new(leaf, 0)), cx)
                .is_ok();
            state.reveal_caret(cx);
            state.focus(window, cx);
            moved.then_some(leaf)
        });
        let Some(leaf) = leaf else {
            return false;
        };
        self.bring_to_top(BlockPosition::new(leaf, 0), cx);
        true
    }

    /// Brings `at` near the top of the page once it's drawn, as Notion does for a heading picked
    /// from a table of contents or a link to a block: below it, the rest of its section.
    fn bring_to_top(&mut self, at: BlockPosition, cx: &mut Context<Self>) {
        self.go_to_top = Some((at, GO_TO_TRIES, Place::Top));
        cx.notify();
    }

    /// Brings `at` to the middle of the page once it's drawn, when it isn't in view now: a
    /// match the find bar steps to, as a browser's find shows it, with the lines around it.
    pub(crate) fn bring_to_middle_if_hidden(&mut self, at: BlockPosition, cx: &mut Context<Self>) {
        let view = self.scroll.bounds();
        let shown = self
            .state
            .read(cx)
            .position_bounds(at)
            .is_some_and(|line| line.top() >= view.top() && line.bottom() <= view.bottom());
        if !shown {
            self.go_to_top = Some((at, GO_TO_TRIES, Place::Middle));
            cx.notify();
        }
    }

    /// Scrolls the block gone to near the top, once it's been drawn (it may have been out of
    /// the drawn part of a long page, and the caret's reveal draws it first).
    fn finish_go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((at, frames, place)) = self.go_to_top else {
            return;
        };
        // The caret's reveal scrolls in the first frame and draws the block in the second: its
        // place is read once that frame has been drawn, and a few frames later at most.
        let drawn = self.state.read(cx).position_bounds(at);
        let Some(line) = drawn.filter(|_| frames <= GO_TO_TRIES - 2) else {
            self.go_to_top = frames.checked_sub(1).map(|f| (at, f, place));
            window.request_animation_frame();
            return;
        };
        self.go_to_top = None;
        let view = self.scroll.bounds();
        let max = self.scroll.max_offset().y;
        let offset = self.scroll.offset();
        let to = match place {
            Place::Top => view.top() + px(GO_TO_MARGIN),
            Place::Middle => view.center().y - line.size.height / 2.,
        };
        let y = (offset.y + to - line.top()).clamp(-max, px(0.));
        if (y - offset.y).abs() >= px(1.) {
            self.scroll.set_offset(point(offset.x, y));
            window.refresh();
        }
    }

    /// Makes the one embed whose block has the lasting id `block` (as [`docs::Embed::block`] and
    /// [`Editor::document`] give it) show `view`, keeping its document, title and height, as one
    /// undo step: for remembering, with the page, the view someone switched to inside that
    /// embed, leaving other embeds of the same document as they are. `view` follows
    /// [`docs::embed`]'s rule for views. Returns false, changing nothing, for an id the document
    /// doesn't have, a block that isn't an embed, the view it already shows, or a read-only
    /// editor.
    pub fn set_embed_view_of(&mut self, block: &str, view: &str, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.lock_ids().block_of(self.state.read(cx).model(), block) else {
            return false;
        };
        let ids = self.ids.clone();
        self.state.update(cx, |state, cx| {
            if state.is_readonly() {
                return false;
            }
            let Some(markdown) = state
                .model()
                .document()
                .block(target)
                .filter(|b| b.kind() == BlockKind::Custom)
                .map(|b| b.markdown())
            else {
                return false;
            };
            let Some((id, title, shown, height)) = docs::embed_of(&markdown) else {
                return false;
            };
            let markdown = docs::embed(&id, &title, view, height);
            if docs::embed_of(&markdown).is_some_and(|(_, _, now, _)| now == shown) {
                return false;
            }
            replace_keeping_id(state, &ids, target, markdown, cx)
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
        let ids = self.ids.clone();
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
                        replace_keeping_id(state, &ids, *block, markdown.clone(), cx)
                    })
                    .count()
            })
        })
    }

    /// Whether the document can be edited.
    pub fn is_readonly(&self, cx: &App) -> bool {
        self.state.read(cx).is_readonly()
    }

    /// Pastes the clipboard's HTML as Markdown (see [`Options::html_clipboard`]) into the focused
    /// document. Returns false, and pastes nothing, when there's no HTML, it holds no text, or
    /// the document can't take it: the document's own paste then pastes the plain text.
    fn paste_html(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(read) = self.html_clipboard.clone() else {
            return false;
        };
        {
            let state = self.state.read(cx);
            if state.is_readonly() || !state.focus_handle(cx).is_focused(window) {
                return false;
            }
            // A web address over selected words links them: the editor's own paste does that.
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            if text.is_some_and(|text| state.pastes_as_link(&text)) {
                return false;
            }
        }
        let Some(markdown) = read(cx)
            .filter(|html| html.len() <= MAX_PASTED_HTML)
            .and_then(|html| crate::html::to_markdown(&html))
        else {
            return false;
        };
        self.state
            .update(cx, |state, cx| state.paste_markdown(&markdown, cx));
        true
    }

    /// ⌘A's first step: with the caret (or part of one line's text) in a page of more than
    /// one line, the whole of that line's text is selected. Returns false when it's selected
    /// already, or there's nothing smaller than the page to select, for ⌘A to select it all.
    fn select_block_text(&mut self, cx: &mut Context<Self>) -> bool {
        self.state.update(cx, |state, cx| {
            let model = state.model();
            let BlockSelection::Text { anchor, head } = *model.selection() else {
                return false;
            };
            let doc = model.document();
            if anchor.block != head.block || doc.leaves().nth(1).is_none() {
                return false;
            }
            let Some(len) = doc.block(head.block).and_then(|b| b.text()).map(str::len) else {
                return false;
            };
            let (from, to) = (
                anchor.offset.min(head.offset),
                anchor.offset.max(head.offset),
            );
            if from == 0 && to == len {
                return false;
            }
            state
                .set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(head.block, 0),
                        head: BlockPosition::new(head.block, len),
                    },
                    cx,
                )
                .is_ok()
        })
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
            .then(|| paste_offer(&self.state, self.docs.as_deref(), self.unfurl.is_some(), cx))
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
            PasteAs::Bookmark => {
                if let Some(url) = &offer.web {
                    self.insert_bookmark(url, cx);
                }
            }
            PasteAs::Mention => self.insert_mention(&offer.doc, &offer.title, cx),
            PasteAs::Page => self.insert_page(&offer.doc, &offer.title, cx),
            PasteAs::Embed => self.insert_embed(&offer.doc, &offer.title, docs::DEFAULT_VIEW, cx),
        }
    }

    /// The offer's menu, under the pasted line.
    fn paste_menu(&self, touch: bool, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
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
        if offer.web.is_some() {
            kinds.push((PasteAs::Bookmark, "Bookmark"));
        } else {
            if offer.plain {
                kinds.push((PasteAs::Mention, "Mention"));
            }
            kinds.push((PasteAs::Page, "Page"));
            kinds.push((PasteAs::Embed, "Embed"));
        }
        let items = kinds.into_iter().map(move |(kind, label)| {
            let this = this.clone();
            div()
                .id(label)
                .debug_selector(move || format!("blocks-paste-{label}"))
                .control(
                    module_path!(),
                    Role::Button,
                    format!("Paste as {}", label.to_lowercase()),
                )
                .px(px(10.))
                .py(px(5.))
                // A finger's target on touch.
                .when(touch, |d| d.h(px(44.)).flex().items_center())
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
        self.insert_at_caret(markdown, block, cx);
    }

    /// Inserts `markdown` at the caret, in place of the selection; as a `block` on a line of
    /// its own: this one when it's empty, else a new one after the caret's top-level block.
    /// Whether it went in.
    fn insert_at_caret(&mut self, markdown: String, block: bool, cx: &mut Context<Self>) -> bool {
        self.state.update(cx, |state, cx| {
            if state.is_readonly() {
                return false;
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
                        return false;
                    };
                    let _ = state.run(EditorCommand::InsertParagraphAfter(top), cx);
                }
                state
                    .run(EditorCommand::InsertMarkdown(markdown), cx)
                    .is_ok()
            })
        })
    }

    /// Inserts a bookmark of the web page at `url` at the caret, on a line of its own as
    /// [`Editor::insert_page`] does, and asks [`Options::unfurl`] about the page: the card
    /// shows the address until the answer comes. Nothing for an address that isn't `http` or
    /// `https`, or in a read-only editor. Whether it went in.
    pub fn insert_bookmark(&mut self, url: &str, cx: &mut Context<Self>) -> bool {
        let url = url.trim();
        if !crate::bookmark::web_url(url) {
            return false;
        }
        let bare = Bookmark::new(url).markdown();
        if !self.insert_at_caret(bare.clone(), true, cx) {
            return false;
        }
        self.unfurl_bookmark(url.to_string(), bare, cx);
        true
    }

    /// Asks the app about the page of the bookmark written `bare`, then writes what it says
    /// into that bookmark, if it's still there and still bare.
    fn unfurl_bookmark(&mut self, url: String, bare: String, cx: &mut Context<Self>) {
        let Some(unfurl) = self.unfurl.clone() else {
            return;
        };
        let answer = unfurl(&url, cx);
        cx.spawn(async move |this, cx| {
            let Some(answer) = answer.await else {
                return;
            };
            let markdown = Bookmark::new(url).unfurled(answer).markdown();
            if markdown == bare {
                return;
            }
            // Not while blocks are dragged: an edit would drop the drag.
            while this
                .update(cx, |this, cx| this.state.read(cx).is_dragging_block())
                .unwrap_or(false)
            {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
            let _ = this.update(cx, |this, cx| {
                let ids = this.ids.clone();
                this.state.update(cx, |state, cx| {
                    if state.is_readonly() {
                        return;
                    }
                    if let Some(block) = custom_block(state, &bare) {
                        replace_keeping_id(state, &ids, block, markdown, cx);
                        // The answer can come after the reader scrolled away: stay there.
                        state.keep_view();
                    }
                });
            });
        })
        .detach();
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
        let at = BlockPosition::new(block, 0);
        let moved = self.state.update(cx, |state, cx| {
            let moved = state.set_selection(BlockSelection::caret(at), cx).is_ok();
            if moved {
                state.reveal_caret(cx);
            }
            moved
        });
        if moved {
            self.bring_to_top(at, cx);
        }
        self.focus(window, cx);
    }

    /// The blocks' lasting ids, for reading or giving.
    pub(crate) fn lock_ids(&self) -> std::sync::MutexGuard<'_, crate::json::Ids> {
        self.ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        let mut gone_for_good = None;
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
                    // An open menu whose query stopped matching closes for good, so the words
                    // typed after a slash ("/so we could…") never reopen it, and Enter is Enter.
                    if open && matches.is_empty() {
                        gone_for_good = Some((caret.block, at));
                    }
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
            self.dismissed = gone_for_good.or_else(|| {
                self.dismissed
                    .filter(|_| self.state_has_slash_at_dismissed(cx))
            });
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

    /// Page Down (`down`) or Up: scrolls a page, less a few lines kept in view, and once that's
    /// drawn puts the caret where it was in the window, extending the selection when `extend`.
    /// With no page left to scroll it goes to the document's last or first line.
    fn page(&mut self, down: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let (head, anchor) = match *state.read(cx).model().selection() {
            BlockSelection::Text { anchor, head } => (head, anchor),
            _ => return,
        };
        let Some(at) = state.read(cx).position_bounds(head) else {
            return;
        };
        let view = self.scroll.bounds().size.height;
        let step = (view - px(PAGE_OVERLAP)).max(view / 2.);
        let offset = self.scroll.offset();
        let max = self.scroll.max_offset().y;
        let y = if down {
            (offset.y - step).max(-max)
        } else {
            (offset.y + step).min(px(0.))
        };
        if (y - offset.y).abs() < px(1.) {
            // Nothing left to scroll: the first or last line, as a text field's Page keys go.
            state.update(cx, |s, cx| {
                let doc = s.model().document();
                let end = if down {
                    doc.leaves()
                        .filter(|l| l.text().is_some())
                        .last()
                        .map(|l| BlockPosition::new(l.id(), l.text().map_or(0, str::len)))
                } else {
                    doc.leaves()
                        .find(|l| l.text().is_some())
                        .map(|l| BlockPosition::new(l.id(), 0))
                };
                if let Some(end) = end {
                    let anchor = if extend { anchor } else { end };
                    let _ = s.set_selection(BlockSelection::Text { anchor, head: end }, cx);
                }
            });
            return;
        }
        self.scroll.set_offset(point(offset.x, y));
        // Placed once the blocks scrolled into view are laid out, in the frame after next.
        self.page_to = Some((at.center(), extend.then_some(anchor), 1));
        window.refresh();
        cx.notify();
    }

    /// Puts the caret where Page Up or Down scrolled to, once that's been drawn.
    fn finish_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, anchor, frames)) = self.page_to.as_mut() else {
            return;
        };
        if *frames > 0 {
            *frames -= 1;
            window.request_animation_frame();
            return;
        }
        let (target, anchor) = (*target, *anchor);
        self.page_to = None;
        self.state.update(cx, |s, cx| {
            let Some(head) = s.position_at(target) else {
                return;
            };
            let anchor = anchor.unwrap_or(head);
            let _ = s.set_selection(BlockSelection::Text { anchor, head }, cx);
        });
    }

    /// ⌘/: the block menu for the caret's block, or the selected blocks; on a phone, or after
    /// a touch, the block sheet.
    fn block_menu_by_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).is_readonly() {
            return;
        }
        let Some(block) = self.key_block(cx) else {
            return;
        };
        if self.shows_side_menu(window) {
            self.block_menu = Some(block);
        } else {
            self.block_sheet = Some(block);
        }
        cx.notify();
    }

    /// The block a key acts on: the first of the selected blocks, else the top-level block the
    /// text selection starts in.
    fn key_block(&self, cx: &App) -> Option<BlockId> {
        match *self.state.read(cx).model().selection() {
            BlockSelection::Blocks { anchor, .. } => Some(anchor),
            _ => self.selection_top_block(cx),
        }
    }

    /// ⌘↵: checks or unchecks the caret's to-do, or opens or closes the toggle whose title the
    /// caret is in. Elsewhere it's the engine's ⌘↵ (leaving a table, or submitting).
    fn check_or_fold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done = self.state.update(cx, |s, cx| {
            let Some(caret) = s.model().caret() else {
                return false;
            };
            let doc = s.model().document();
            let mut at = doc.block(caret.block);
            let mut child = None;
            while let Some(b) = at {
                match b.kind() {
                    BlockKind::ListItem {
                        checked: Some(checked),
                    } => {
                        if s.is_readonly() {
                            return false;
                        }
                        return s.run(EditorCommand::SetTaskChecked(!checked), cx).is_ok();
                    }
                    // In its title, its first block: open or closed, as a click on the chevron.
                    BlockKind::Toggle { open } if b.children().next().map(|c| c.id()) == child => {
                        let toggle = b.id();
                        return s
                            .run(
                                EditorCommand::SetToggleOpen {
                                    block: toggle,
                                    open: !open,
                                },
                                cx,
                            )
                            .is_ok();
                    }
                    _ => {}
                }
                child = Some(b.id());
                at = b.parent();
            }
            false
        });
        if !done {
            cx.propagate();
        }
        let _ = window;
    }

    /// ⌥↵: opens what's at the caret as a click would: a link, a mention, or the caret's
    /// sub-page, embed or bookmark.
    fn open_at_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let event = {
            let state = self.state.read(cx);
            if let Some(url) = state.link_at_caret() {
                Some(EditorEvent::LinkClicked { url })
            } else if let Some(node) = state.custom_at_caret() {
                if let Some(doc) = node.data::<docs::DocRef>() {
                    Some(EditorEvent::DocClicked { id: doc.id.clone() })
                } else {
                    node.data::<crate::mentions::MentionRef>().map(|m| {
                        EditorEvent::MentionClicked {
                            kind: m.kind,
                            id: m.id.clone(),
                        }
                    })
                }
            } else {
                self.key_block(cx)
                    .and_then(|block| self.block_target(block, cx))
            }
        };
        match event {
            Some(event) => cx.emit(event),
            None => cx.propagate(),
        }
    }

    /// What opening the custom block `block` asks the app for: its document or its page.
    pub(crate) fn block_target(&self, block: BlockId, cx: &App) -> Option<EditorEvent> {
        let state = self.state.read(cx);
        let b = state.model().document().block(block)?;
        if b.kind() != BlockKind::Custom {
            return None;
        }
        match crate::json::kind_of_custom(&b.markdown()) {
            crate::json::Kind::Page { doc, .. } | crate::json::Kind::Embed { doc, .. } => {
                Some(EditorEvent::DocClicked { id: doc.into() })
            }
            crate::json::Kind::Bookmark { url, .. } => {
                Some(EditorEvent::LinkClicked { url: url.into() })
            }
            _ => None,
        }
    }

    /// ⌘⇧M: a comment on the selection, as the format bar's Comment asks for one. The key is
    /// the editor's when it takes comments (the app's own ⌘⇧M doesn't run too).
    fn comment_by_key(&mut self, cx: &mut Context<Self>) {
        if !self.comments {
            cx.propagate();
            return;
        }
        if let Some((block, quote)) = self.selection_anchor(cx) {
            cx.emit(EditorEvent::CommentRequested { block, quote });
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
            SlashAction::Bookmark => Some(EditorEvent::BookmarkRequested),
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
                    SlashAction::ToggleHeading(level) => {
                        let level = (*level).clamp(1, 3);
                        let _ = state.run(
                            EditorCommand::SetBlockKind(BlockKind::Heading { level }),
                            cx,
                        );
                        let _ = state.run(EditorCommand::WrapInToggle, cx);
                    }
                    SlashAction::Color { text, index } => paint_line(state, *text, *index, cx),
                    // The app answers with insert_page or insert_mention.
                    SlashAction::NewPage
                    | SlashAction::LinkToPage
                    | SlashAction::Embed
                    | SlashAction::Bookmark => {}
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

    /// Whether controls are sized for a finger, 44 points each way: on a phone's layout, or
    /// after a touch.
    pub(crate) fn touch_sized(&self, window: &Window) -> bool {
        self.compact(window) || window.last_input_was_touch()
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

    /// Scrolls the page a step toward the edge the dragged blocks' pointer is near, faster
    /// the nearer it is, and lets the drag follow. Returns whether it scrolled.
    pub(crate) fn scroll_near_edge(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(at) = self.state.read(cx).block_drag_pointer() else {
            return false;
        };
        let view = self.scroll.bounds();
        let reach = px(EDGE_SCROLL_REACH);
        let step = if at.y < view.top() + reach {
            (view.top() + reach - at.y).min(reach) / reach * EDGE_SCROLL_SPEED
        } else if at.y > view.bottom() - reach {
            -((at.y - (view.bottom() - reach)).min(reach) / reach * EDGE_SCROLL_SPEED)
        } else {
            return false;
        };
        let offset = self.scroll.offset();
        let max = self.scroll.max_offset();
        let y = (offset.y + px(step)).clamp(-max.y, px(0.));
        if y == offset.y {
            return false;
        }
        self.scroll.set_offset(point(offset.x, y));
        self.state
            .update(cx, |state, cx| state.continue_block_drag(at, cx));
        cx.notify();
        true
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
        // In columns: the block under the pointer in the column it's over.
        let inner = hovered.and_then(|columns| {
            let state = self.state.read(cx);
            if state.model().document().block(columns)?.kind() != BlockKind::Columns {
                return None;
            }
            let x = event.position.x;
            let (column, _) = state.children_bounds(columns).into_iter().find(|(_, b)| {
                x >= b.left() - px(30.)
                    && x < b.right()
                    && y >= b.top() - px(6.)
                    && y < b.bottom() + px(6.)
            })?;
            state
                .children_bounds(column)
                .into_iter()
                .find(|(_, b)| y >= b.top() - px(4.) && y < b.bottom() + px(4.))
                .map(|(id, _)| id)
        });
        if hovered != self.hovered || inner != self.hovered_inner {
            self.hovered = hovered;
            self.hovered_inner = inner;
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
        // Beside the block whose menu is open, else the block under the pointer: in columns,
        // the one in the column it's over.
        let block = self
            .block_menu
            .or(self.hovered.map(|h| self.hovered_inner.unwrap_or(h)))?;
        let state = self.state.read(cx);
        if state.is_readonly() {
            return None;
        }
        let blocks = state.top_level_blocks();
        let (bounds, left, inner) = match blocks.iter().find(|(id, _)| *id == block) {
            // The page's left edge: where top-level text starts.
            Some((_, bounds)) => (
                *bounds,
                blocks.iter().map(|(_, b)| b.left()).reduce(Pixels::min)?,
                false,
            ),
            // A block in a column: its grip in the gap left of its column.
            None => {
                let doc = state.model().document();
                let column = doc.block(block)?.parent()?;
                let columns = column.parent()?;
                let (_, column_bounds) = state
                    .children_bounds(columns.id())
                    .into_iter()
                    .find(|(id, _)| *id == column.id())?;
                let (_, bounds) = state
                    .children_bounds(column.id())
                    .into_iter()
                    .find(|(id, _)| *id == block)?;
                (bounds, column_bounds.left(), true)
            }
        };
        let bounds = &bounds;
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
            .debug_selector(|| "blocks-side-grip".into())
            .control(module_path!(), Role::Button, "Move block")
            .aria_description("Drag to move the block; click for its menu")
            .aria_expanded(self.block_menu == Some(block))
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
        if inner {
            return Some(
                deferred(
                    anchored()
                        .position(point(left - px(22.), top))
                        .child(div().flex().child(grip)),
                )
                .with_priority(1),
            );
        }
        Some(
            deferred(
                anchored().position(point(left - px(52.), top)).child(
                    div()
                        .flex()
                        .gap(px(2.))
                        .child(
                            button("blocks-side-add", "+")
                                .control(module_path!(), Role::Button, "Add a block below")
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

    fn slash_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let touch = self.touch_sized(window);
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
                    .debug_selector(move || format!("blocks-slash-{n}"))
                    .control(module_path!(), Role::ListBoxOption, item.title.clone())
                    .aria_description(item.hint.clone())
                    .aria_selected(selected)
                    .when(touch, |d| d.min_h(px(44.)))
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
                    .children(item.shortcut.clone().map(|keys| {
                        div()
                            .ml_auto()
                            .pl(px(8.))
                            .flex_none()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(keys)
                    }))
                    .into_any_element(),
            );
        }
        let height = (px(MENU_ROW) * rows.len() as f32 + px(8.)).min(px(MENU_HEIGHT));
        Some(
            deferred(
                caret_menu(caret, height, window).child(
                    div()
                        .id("blocks-slash-menu")
                        .debug_selector(|| "blocks-slash-menu".into())
                        .control(module_path!(), Role::ListBox, "Blocks")
                        .w(px(300.))
                        .max_h(px(MENU_HEIGHT))
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

/// The tallest the slash and @ menus get, scrolling past it.
pub(crate) const MENU_HEIGHT: f32 = 340.;

/// About how tall one of their rows is, a title and a hint.
pub(crate) const MENU_ROW: f32 = 44.;

/// Where a menu at the caret goes: under the caret's line, or over it when there's no room
/// for the menu's `height` below and more above, as near the bottom of the window, so the menu
/// never covers the line being typed.
pub(crate) fn caret_menu(caret: Bounds<Pixels>, height: Pixels, window: &Window) -> Anchored {
    let x = caret.left() - px(4.);
    let below = window.viewport_size().height - caret.bottom();
    let above = caret.top();
    if below < height + px(14.) && above > below {
        anchored()
            .anchor(Anchor::BottomLeft)
            .position(point(x, caret.top() - px(6.)))
            .snap_to_window_with_margin(px(8.))
    } else {
        anchored()
            .position(point(x, caret.bottom() + px(6.)))
            .snap_to_window_with_margin(px(8.))
    }
}

/// Replaces `block` with `markdown`, and gives the block in its place its lasting id.
pub(crate) fn replace_keeping_id(
    state: &mut BlockEditorState,
    ids: &std::sync::Mutex<crate::json::Ids>,
    block: BlockId,
    markdown: String,
    cx: &mut Context<BlockEditorState>,
) -> bool {
    let place = state.model().document().block(block).and_then(|b| {
        let parent = b.parent()?;
        let index = parent.children().position(|c| c.id() == block)?;
        Some((parent.id(), index))
    });
    if state
        .run(EditorCommand::ReplaceBlock { block, markdown }, cx)
        .is_err()
    {
        return false;
    }
    let now = place.and_then(|(parent, index)| {
        let doc = state.model().document();
        doc.block(parent)?.children().nth(index).map(|b| b.id())
    });
    if let Some(now) = now {
        ids.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .carry(block, now);
    }
    true
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

/// The first custom block written as `markdown`, in document order.
fn custom_block(state: &BlockEditorState, markdown: &str) -> Option<BlockId> {
    fn walk(block: markdown_editor::BlockRef<'_>, markdown: &str) -> Option<BlockId> {
        if block.kind() == BlockKind::Custom && block.markdown() == markdown {
            return Some(block.id());
        }
        block.children().find_map(|child| walk(child, markdown))
    }
    walk(state.model().document().root(), markdown)
}

/// What a pasted document link or web address can become.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PasteAs {
    Mention,
    Page,
    Embed,
    Bookmark,
}

/// A line holding only a pasted document link.
#[derive(Clone, Debug)]
struct PasteOffer {
    block: BlockId,
    doc: String,
    title: String,
    /// Pasted as the text `doc:id`, not yet a mention.
    plain: bool,
    /// A web address pasted in place of a document link: it can become a bookmark.
    web: Option<String>,
}

/// The offer for the caret's line, when it's a paragraph holding only a document link.
fn paste_offer(
    state: &Entity<BlockEditorState>,
    docs: Option<&dyn Docs>,
    bookmarks: bool,
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
    // A web address alone on its line, as text or a link.
    if bookmarks && crate::bookmark::web_url(text) {
        return Some(PasteOffer {
            block: caret.block,
            doc: String::new(),
            title: String::new(),
            plain: true,
            web: Some(text.to_string()),
        });
    }
    // Text from another app: `[title](doc:id)` pasted as it was written.
    if let Some((doc, title)) = docs::mention_of(text) {
        return Some(PasteOffer {
            block: caret.block,
            doc,
            title,
            plain: true,
            web: None,
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
            web: None,
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
        web: None,
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
        let readonly = state.is_readonly();
        // Above the selection, as iOS's; below it when there's no room above (the selection near
        // the top of the page, or under the status bar), unless that's behind the keyboard.
        let height = if readonly { px(46.) } else { px(92.) };
        let visible = window.fully_visible_bounds();
        let above = point(x, first.top() - px(18.));
        let below = point(x, last.bottom() + px(11. + 18.));
        let flip = above.y - height < visible.top() + px(8.)
            && below.y + height <= visible.bottom() - px(8.);
        let (at, corner) = if flip {
            (below, Anchor::TopCenter)
        } else {
            (above, Anchor::BottomCenter)
        };
        // The format bar's marks and Link, as a second row.
        let format_row = (!readonly).then(|| {
            let mut row = self.format_buttons(px(44.), cx);
            // Color: the colours in a sheet, for the selection.
            let theme = cx.theme();
            let this = cx.entity().downgrade();
            row.push(
                div()
                    .id("blocks-touch-color")
                    .debug_selector(|| "blocks-touch-color".into())
                    .control(module_path!(), Role::Button, "Color")
                    .size(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(17.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.popover_foreground)
                    .child("A")
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let _ = this.update(cx, |this, cx| {
                            this.used_menu(cx);
                            this.color_sheet = true;
                            cx.notify();
                        });
                    })
                    .into_any_element(),
            );
            row
        });
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
                .control(module_path!(), Role::MenuItem, label)
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
                    .control(module_path!(), Role::MenuItem, "Block menu")
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
                    .anchor(corner)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("blocks-edit-menu")
                            .control(module_path!(), Role::Menu, "Edit")
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
        self.finish_page(window, cx);
        self.finish_go_to(window, cx);
        let compact = self.compact(window);
        let side_menu = if self.shows_side_menu(window) {
            self.side_menu(cx)
        } else {
            None
        };
        let (gutter, top) = if compact {
            (px(COMPACT_SIDE), px(COMPACT_INSET))
        } else {
            (self.gutter, self.top)
        };
        let slash_menu = self.slash_menu(window, cx);
        let mention_menu = self.mention_menu(window, cx);
        let handles = self.selection_handles(window, cx);
        let touch = self.touch_sized(window);
        let paste_menu = self.paste_menu(touch, cx);
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
        let code_chrome = self.code_chrome(touch, cx);
        let column_handles = if self.shows_side_menu(window) {
            self.column_handles(cx)
        } else {
            None
        };
        let block_sheet = self.block_sheet(cx);
        let color_sheet = self.color_sheet(cx);
        let find_bar = self.find_bar(compact, touch, cx);
        // While a grip is pressed: its release, seen before the editor ends the drag.
        // While blocks are dragged, the page scrolls when the pointer (or finger) nears its top
        // or bottom edge, a frame at a time. A step asks for the next frame only when it
        // scrolled, so a block held still asks for none.
        let edge_scroll = self.state.read(cx).block_drag_pointer().map(|_| {
            let this = cx.entity().downgrade();
            canvas(
                |_, _, _| {},
                move |_, _, window, cx| {
                    let this = this.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = this.update(cx, |this, cx| this.scroll_near_edge(cx));
                    });
                },
            )
            .size_0()
        });
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
        // Room below the last line: enough to scroll it above a phone's software keyboard when
        // one covers the page.
        let keyboard = window.viewport_size().height - window.fully_visible_bounds().bottom();
        let bottom_room = px(PAGE_BOTTOM).max(keyboard + px(96.));
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
            .on_action(cx.listener(|this, _: &HighlightAgain, _, cx| this.highlight_again(cx)))
            .on_action(cx.listener(|this, _: &PageDown, w, cx| this.page(true, false, w, cx)))
            .on_action(cx.listener(|this, _: &PageUp, w, cx| this.page(false, false, w, cx)))
            .on_action(cx.listener(|this, _: &SelectPageDown, w, cx| this.page(true, true, w, cx)))
            .on_action(cx.listener(|this, _: &SelectPageUp, w, cx| this.page(false, true, w, cx)))
            .on_action(cx.listener(|this, _: &OpenBlockMenu, w, cx| this.block_menu_by_key(w, cx)))
            .on_action(cx.listener(|this, _: &CheckOrFold, w, cx| this.check_or_fold(w, cx)))
            .on_action(cx.listener(|this, _: &OpenAtCaret, w, cx| this.open_at_caret(w, cx)))
            .on_action(cx.listener(|this, _: &Comment, _, cx| this.comment_by_key(cx)))
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
                            .pb(bottom_room),
                    ),
            )
            .children(side_menu)
            .children(code_chrome)
            .children(column_handles)
            .children(slash_menu)
            .children(mention_menu)
            .children(handles)
            .children(paste_menu)
            .children(resize)
            .children(edit_menu)
            .children(format_bar)
            .child(block_sheet)
            .child(color_sheet)
            .children(grip_watch)
            .children(edge_scroll)
            .child(self.long_press_watch(cx));
        // The find bar stays at the top while the page scrolls under it.
        div()
            .relative()
            .size_full()
            // Before the document's own paste: the clipboard's HTML, when the app reads it.
            // ⌘A in two steps, as Notion's: the caret's block's text first, then everything.
            // The touch menu's Select all selects everything at once, as iOS's does.
            .capture_action(cx.listener(
                |this, _: &markdown_editor::actions::SelectAll, window, cx| {
                    if !window.last_input_was_touch() && this.select_block_text(cx) {
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_action(
                cx.listener(|this, _: &markdown_editor::actions::Paste, window, cx| {
                    if this.paste_html(window, cx) {
                        cx.stop_propagation();
                    }
                }),
            )
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
    use super::{COMPACT_INSET, COMPACT_SIDE, Editor, GUTTER, Options, TOP};
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use markdown_editor::{BlockId, BlockKind, BlockPosition, BlockSelection};
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

    /// What a click does, by keyboard: ⌘/ opens the block menu, ⌘↵ checks a to-do and folds a
    /// toggle, ⌥↵ opens a link, a sub-page or a bookmark, ⌘⇧M asks for a comment, and the
    /// block menu's Open and Copy code do what clicks on the block do.
    #[gpui::test]
    fn the_keyboard_reaches_what_clicks_do(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let source = "- [ ] Buy milk\n\n<details open>\n<summary>More</summary>\n\nInside\n\n</details>\n\nSee [the site](https://example.com) now.\n\n[Roadmap](doc:r1 \"page\")\n\n```rust\nfn main() {}\n```";
        let (editor, cx) = cx.add_window_view(move |window, cx| {
            Editor::with_options(source, super::Options::default().comments(), window, cx)
        });
        let events: std::rc::Rc<std::cell::RefCell<Vec<super::EditorEvent>>> = Default::default();
        let seen = events.clone();
        cx.update(|window, cx| {
            cx.subscribe(&editor, move |_, e: &super::EditorEvent, _| {
                if !matches!(e, super::EditorEvent::Changed) {
                    seen.borrow_mut().push(e.clone());
                }
            })
            .detach();
            editor.update(cx, |e, cx| e.focus(window, cx));
        });
        draw(cx);
        let caret_in =
            |editor: &Entity<Editor>, cx: &mut VisualTestContext, leaf: usize, at: usize| {
                editor.update(cx, |e, cx| {
                    e.state().update(cx, |s, cx| {
                        let id = s.model().document().leaves().nth(leaf).unwrap().id();
                        let _ =
                            s.set_selection(BlockSelection::caret(BlockPosition::new(id, at)), cx);
                    })
                });
                draw(cx);
            };
        // ⌘↵ checks the to-do, and again unchecks it.
        caret_in(&editor, cx, 0, 3);
        cx.simulate_keystrokes("secondary-enter");
        draw(cx);
        assert!(
            markdown(&editor, cx).starts_with("- [x] Buy milk"),
            "{}",
            markdown(&editor, cx)
        );
        cx.simulate_keystrokes("secondary-enter");
        draw(cx);
        assert!(markdown(&editor, cx).starts_with("- [ ] Buy milk"));
        // In a toggle's title, ⌘↵ closes it.
        caret_in(&editor, cx, 1, 2);
        cx.simulate_keystrokes("secondary-enter");
        draw(cx);
        assert!(
            markdown(&editor, cx).contains("<details>\n<summary>More"),
            "{}",
            markdown(&editor, cx)
        );
        // ⌥↵ in a link opens it.
        let link = editor.update(cx, |e, cx| {
            let doc = e.state().read(cx).model().document();
            doc.leaves()
                .position(|l| l.text().is_some_and(|t| t.starts_with("See")))
                .unwrap()
        });
        caret_in(&editor, cx, link, 6);
        cx.simulate_keystrokes("alt-enter");
        draw(cx);
        assert!(
            events.borrow().iter().any(|e| matches!(
                e,
                super::EditorEvent::LinkClicked { url } if url.as_ref() == "https://example.com"
            )),
            "{:?}",
            events.borrow()
        );
        // ⌘⇧M asks for a comment on the selection.
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let id = s.model().document().leaves().nth(link).unwrap().id();
                let _ = s.set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(id, 0),
                        head: BlockPosition::new(id, 3),
                    },
                    cx,
                );
            })
        });
        cx.simulate_keystrokes("secondary-shift-m");
        draw(cx);
        assert!(
            events.borrow().iter().any(|e| matches!(
                e,
                super::EditorEvent::CommentRequested { quote, .. } if quote == "See"
            )),
            "{:?}",
            events.borrow()
        );
        // ⌘/ opens the block menu; its Open opens the sub-page.
        caret_in(&editor, cx, link, 0);
        cx.simulate_keystrokes("secondary-/");
        draw(cx);
        let top = editor.update(cx, |e, cx| e.selection_top_block(cx));
        assert_eq!(editor.read_with(cx, |e, _| e.block_menu), top);
        let (page, code) = editor.update(cx, |e, cx| {
            let doc = e.state().read(cx).model().document();
            let blocks: Vec<_> = doc.root().children().map(|b| (b.id(), b.kind())).collect();
            let page = blocks
                .iter()
                .find(|(_, k)| *k == BlockKind::Custom)
                .unwrap()
                .0;
            let code = blocks
                .iter()
                .find(|(_, k)| *k == BlockKind::CodeBlock)
                .unwrap()
                .0;
            (page, code)
        });
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(page, "open", window, cx)
        });
        assert!(
            events.borrow().iter().any(|e| matches!(
                e,
                super::EditorEvent::DocClicked { id } if id.as_ref() == "r1"
            )),
            "{:?}",
            events.borrow()
        );
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(code, "copy-code", window, cx)
        });
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some("fn main() {}")
        );
    }

    /// A heading picked from the table of contents, far down a long page, lands near the top
    /// of the page, as Notion's does, not at its bottom edge.
    #[gpui::test]
    fn a_heading_gone_to_lands_near_the_top(cx: &mut TestAppContext) {
        let mut parts = vec![crate::toc::MARKDOWN.to_string()];
        for section in 1..=6 {
            parts.push(format!("## Part {section}"));
            for line in 1..=30 {
                parts.push(format!("Line {line} of part {section}"));
            }
        }
        let source = parts.join("\n\n");
        let (editor, cx) = open(cx, &source);
        let target = cx
            .debug_bounds("blocks-toc-4")
            .expect("the contents")
            .center();
        cx.simulate_click(target, gpui::Modifiers::none());
        for _ in 0..8 {
            draw(cx);
        }
        let (heading, view) = editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            let heading = state
                .model()
                .document()
                .leaves()
                .find(|l| l.text() == Some("Part 5"))
                .unwrap()
                .id();
            (
                state
                    .position_bounds(BlockPosition::new(heading, 0))
                    .unwrap(),
                e.scroll.bounds(),
            )
        });
        assert!(
            heading.top() >= view.top() && heading.top() < view.top() + gpui::px(100.),
            "near the top: {heading:?} in {view:?}"
        );
    }

    /// On a phone every control is a finger's target, 44 points each way: the slash menu's
    /// rows, a code block's chrome and the find bar's buttons.
    #[gpui::test]
    fn a_phones_controls_are_a_fingers_size(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "Words\n\n```rust\nfn main() {}\n```");
        cx.simulate_resize(gpui::size(gpui::px(390.), gpui::px(844.)));
        draw(cx);
        let tall = |cx: &mut VisualTestContext, name: &'static str| {
            let b = cx
                .debug_bounds(name)
                .unwrap_or_else(|| panic!("{name} isn't drawn"));
            assert!(b.size.height >= gpui::px(44.), "{name}: {b:?}");
        };
        // The slash menu, on a new line after the words.
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let id = s.model().document().leaves().next().unwrap().id();
                let _ = s.set_selection(BlockSelection::caret(BlockPosition::new(id, 5)), cx);
            })
        });
        cx.simulate_keystrokes("enter");
        cx.simulate_input("/");
        draw(cx);
        tall(cx, "blocks-slash-0");
        cx.simulate_keystrokes("escape");
        // The code block's chrome, with the caret in it.
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let id = s
                    .model()
                    .document()
                    .leaves()
                    .find(|l| l.kind() == BlockKind::CodeBlock)
                    .unwrap()
                    .id();
                let _ = s.set_selection(BlockSelection::caret(BlockPosition::new(id, 2)), cx);
            })
        });
        draw(cx);
        draw(cx);
        tall(cx, "blocks-code-copy");
        // The find bar.
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-f"
        } else {
            "ctrl-f"
        });
        draw(cx);
        tall(cx, "find-next");
    }

    /// On a phone the caret stays above the software keyboard as you type, the last line too:
    /// the page has room below to scroll it up.
    #[gpui::test]
    fn the_caret_stays_above_the_keyboard(cx: &mut TestAppContext) {
        let source = (1..=60)
            .map(|n| format!("Line {n}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let (editor, cx) = open(cx, &source);
        cx.simulate_resize(gpui::size(gpui::px(402.), gpui::px(874.)));
        draw(cx);
        // The keyboard comes up over the bottom 340 points.
        let keyboard_top = gpui::px(874. - 340.);
        let window = gpui::VisualContext::window_handle(cx);
        cx.simulate_window_visual_viewport_change(
            window,
            gpui::Bounds::new(
                gpui::point(gpui::px(0.), gpui::px(0.)),
                gpui::size(gpui::px(402.), keyboard_top),
            ),
        );
        draw(cx);
        let caret_bottom = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| {
                e.state().read(cx).caret_bounds().unwrap().bottom()
            })
        };
        for leaf in [16, 59] {
            // A line under where the keyboard is (and the last), then a letter typed there.
            editor.update(cx, |e, cx| {
                e.state().update(cx, |s, cx| {
                    let id = s.model().document().leaves().nth(leaf).unwrap().id();
                    let _ = s.set_selection(BlockSelection::caret(BlockPosition::new(id, 4)), cx);
                })
            });
            cx.simulate_input("x");
            for _ in 0..4 {
                draw(cx);
            }
            let bottom = caret_bottom(&editor, cx);
            assert!(
                bottom <= keyboard_top,
                "line {leaf}: caret at {bottom:?}, keyboard at {keyboard_top:?}"
            );
        }
    }

    /// Page Down and Up move the caret a page with the view, ⇧ extending the selection; past
    /// the last page, to the last line.
    #[gpui::test]
    fn page_keys_move_the_caret_a_page(cx: &mut TestAppContext) {
        let source = (1..=300)
            .map(|n| format!("Line {n}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let (editor, cx) = open(cx, &source);
        let line = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| {
                let state = e.state().read(cx);
                let doc = state.model().document();
                let BlockSelection::Text { head, .. } = *state.model().selection() else {
                    panic!("a text selection");
                };
                doc.leaves().position(|l| l.id() == head.block).unwrap()
            })
        };
        let page = |cx: &mut VisualTestContext, key: &str| {
            cx.simulate_keystrokes(key);
            for _ in 0..3 {
                draw(cx);
            }
        };
        assert_eq!(line(&editor, cx), 0);
        page(cx, "pagedown");
        let one = line(&editor, cx);
        assert!(one > 5, "a page down: line {one}");
        let offset = editor.read_with(cx, |e, _| e.scroll.offset().y);
        assert!(offset < gpui::px(-100.), "scrolled: {offset:?}");
        page(cx, "pagedown");
        let two = line(&editor, cx);
        assert!(two > one + 5, "another page: line {two}");
        page(cx, "pageup");
        let back = line(&editor, cx);
        assert!(
            back < two && back + 3 >= one && back <= one + 3,
            "back up: {back} after {one}"
        );
        // ⇧ extends from where it was.
        page(cx, "shift-pagedown");
        editor.update(cx, |e, cx| {
            let selection = e.state().read(cx).model().selection().clone();
            assert!(!selection.is_collapsed(), "{selection:?}");
        });
        // Up past the top: the first line; down past the end: the last.
        for _ in 0..40 {
            page(cx, "pageup");
        }
        assert_eq!(line(&editor, cx), 0);
        for _ in 0..80 {
            page(cx, "pagedown");
        }
        assert_eq!(line(&editor, cx), 299);
    }

    /// Near the bottom of the window the slash menu opens over the line, not on it.
    #[gpui::test]
    fn the_slash_menu_opens_above_a_line_near_the_bottom(cx: &mut TestAppContext) {
        let source = (1..=200)
            .map(|n| format!("Line {n}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let (editor, cx) = open(cx, &source);
        let bottom = cx.update(|window, _| window.viewport_size().height);
        // The last line drawn wholly in the window.
        let (leaf, len) = editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            state
                .model()
                .document()
                .leaves()
                .filter_map(|l| {
                    let len = l.text()?.len();
                    let at = state.position_bounds(BlockPosition::new(l.id(), len))?;
                    (at.bottom() < bottom - gpui::px(8.)).then_some((l.id(), len))
                })
                .last()
                .unwrap()
        });
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let _ = s.set_selection(BlockSelection::caret(BlockPosition::new(leaf, len)), cx);
            })
        });
        cx.simulate_input(" /");
        draw(cx);
        let caret = editor.update(cx, |e, cx| e.state().read(cx).caret_bounds().unwrap());
        let menu = cx.debug_bounds("blocks-slash-menu").expect("the menu");
        assert!(
            menu.bottom() <= caret.top(),
            "menu {menu:?} over the caret {caret:?}"
        );
        // At the top it opens under the line, as before.
        cx.simulate_keystrokes("escape");
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let first = s.model().document().leaves().next().unwrap().id();
                let _ = s.set_selection(BlockSelection::caret(BlockPosition::new(first, 6)), cx);
            })
        });
        draw(cx);
        cx.simulate_input(" /");
        draw(cx);
        let caret = editor.update(cx, |e, cx| e.state().read(cx).caret_bounds().unwrap());
        let menu = cx.debug_bounds("blocks-slash-menu").expect("the menu");
        assert!(
            menu.top() >= caret.bottom(),
            "menu {menu:?} under the caret {caret:?}"
        );
    }

    /// Typing shortcuts: arrows and an em dash, one undo putting back what was typed, never in
    /// code, inline code or a web address, and off with `Options::typing_shortcuts(false)`.
    #[gpui::test]
    fn typing_shortcuts_make_arrows_and_dashes(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("a -> b <- c -- d");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "a → b ← c — d");
        // Undo right after one puts back the characters typed.
        cx.simulate_input(" e ->");
        draw(cx);
        assert!(markdown(&editor, cx).ends_with("e →"));
        cx.simulate_keystrokes("secondary-z");
        draw(cx);
        assert!(
            markdown(&editor, cx).ends_with("e -\\>"),
            "{}",
            markdown(&editor, cx)
        );
        // Not in a web address, inline code or a code block; `---` still makes a divider.
        let (editor, cx) = open(cx, "`x` ");
        caret_at_end(&editor, cx, 0);
        cx.simulate_input("see https://a.b/c->d and ");
        draw(cx);
        assert!(
            !markdown(&editor, cx).contains('→'),
            "{}",
            markdown(&editor, cx)
        );
        let (editor, cx) = open(cx, "```\ncode\n```");
        caret_at_end(&editor, cx, 0);
        cx.simulate_input(" x->y");
        draw(cx);
        assert!(
            markdown(&editor, cx).contains("x->y"),
            "{}",
            markdown(&editor, cx)
        );
        let (editor, cx) = open(cx, "");
        cx.simulate_input("---");
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            markdown(&editor, cx).starts_with("---"),
            "{}",
            markdown(&editor, cx)
        );
    }

    /// `Options::typing_shortcuts(false)` leaves what's typed as it is.
    #[gpui::test]
    fn typing_shortcuts_turn_off(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(|window, cx| {
            Editor::with_options(
                "",
                super::Options::default().typing_shortcuts(false),
                window,
                cx,
            )
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        cx.simulate_input("a -> b");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "a -\\> b");
    }

    /// ⌘A selects the caret's line first, then the whole page, as Notion's does.
    #[gpui::test]
    fn select_all_takes_the_line_then_the_page(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "first\n\nsecond line\n\nthird");
        caret_at_end(&editor, cx, 1);
        let selection = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| e.state().read(cx).model().selection().clone())
        };
        let second = editor.update(cx, |e, cx| {
            e.state()
                .read(cx)
                .model()
                .document()
                .leaves()
                .nth(1)
                .unwrap()
                .id()
        });
        cx.simulate_keystrokes("secondary-a");
        draw(cx);
        assert_eq!(
            selection(&editor, cx),
            BlockSelection::Text {
                anchor: BlockPosition::new(second, 0),
                head: BlockPosition::new(second, 11),
            }
        );
        cx.simulate_keystrokes("secondary-a");
        draw(cx);
        let BlockSelection::Text { anchor, head } = selection(&editor, cx) else {
            panic!("a text selection");
        };
        assert_ne!(anchor.block, head.block, "the whole page");
        // A one-line page: everything at once.
        let (editor, cx) = open(cx, "only");
        caret_at_end(&editor, cx, 0);
        cx.simulate_keystrokes("secondary-a");
        draw(cx);
        let BlockSelection::Text { anchor, head } = selection(&editor, cx) else {
            panic!("a text selection");
        };
        assert_eq!((anchor.offset, head.offset), (0, 4));
    }

    /// Timings on a 5,000-block page, for finding what's slow: run with
    /// `cargo test -p gpui-blocks --release --features gpui/bench-support -- --ignored
    /// long_page_timings --nocapture`. Headless (GPUI's test platform lays out and paints into
    /// nothing), so it measures the editor's own work, not the GPU's.
    #[gpui::test]
    #[ignore]
    fn long_page_timings(cx: &mut TestAppContext) {
        use std::time::{Duration, Instant};
        let mut page = String::new();
        for i in 0..5000 {
            match i % 10 {
                0 => page.push_str(&format!("## Section {i}\n\n")),
                1 | 2 => page.push_str(&format!("- item {i} with **bold**, `code` and a [link](https://example.com/{i})\n")),
                3 => page.push_str(&format!("\n> A quote, number {i}, with *emphasis*.\n\n")),
                4 => page.push_str(&format!("```rust\nfn f{i}() -> u32 {{ {i} }}\n```\n\n")),
                5 => page.push_str(&format!("- [ ] task {i}\n\n")),
                _ => page.push_str(&format!(
                    "Paragraph {i}: the plan is to write, review and ship it, with notes on what changed and why.\n\n"
                )),
            }
        }
        let median = |mut runs: Vec<Duration>| {
            runs.sort();
            runs[runs.len() / 2]
        };
        let t = Instant::now();
        let parsed =
            markdown_editor::BlockEditorModel::from_markdown(&page, crate::extensions(&[]));
        let parse = t.elapsed();
        drop(parsed);
        let t = Instant::now();
        let (editor, cx) = open(cx, &page);
        let made = t.elapsed();
        cx.simulate_resize(gpui::size(gpui::px(1280.), gpui::px(820.)));
        draw(cx);
        let open_time = t.elapsed();
        let leaves = editor.update(cx, |e, cx| {
            e.state().read(cx).model().document().leaves().count()
        });
        // A frame and the work it leaves for after it, as the app's loop runs both.
        let frame = |cx: &mut VisualTestContext| {
            let t = Instant::now();
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            cx.run_until_parked();
            t.elapsed()
        };
        let steady = median((0..9).map(|_| frame(cx)).collect());
        // Typing in the middle, a keystroke and its frame.
        caret_at_end(&editor, cx, leaves / 2);
        let typing = median(
            (0..15)
                .map(|_| {
                    let t = Instant::now();
                    cx.simulate_input("x");
                    draw(cx);
                    t.elapsed()
                })
                .collect(),
        );
        let enter = median(
            (0..9)
                .map(|_| {
                    let t = Instant::now();
                    cx.simulate_keystrokes("enter");
                    draw(cx);
                    t.elapsed()
                })
                .collect(),
        );
        // Scrolling end to end in 40-point steps, a frame each.
        let max = editor.read_with(cx, |e, _| e.scroll.max_offset().y);
        let mut scroll_frames = Vec::new();
        let mut y = gpui::px(0.);
        while y < max {
            y = (y + gpui::px(40.)).min(max);
            editor.read_with(cx, |e, _| {
                e.scroll.set_offset(gpui::point(gpui::px(0.), -y))
            });
            scroll_frames.push(frame(cx));
        }
        let scrolled = scroll_frames.len();
        let scroll_worst = scroll_frames.iter().max().copied().unwrap_or_default();
        let scroll = median(scroll_frames);
        // Select all, copy it, and the save's JSON and Markdown.
        // The test context's first update after thousands of frames settles them (about half a
        // second, which the app's own frame loop never piles up): out of the timings below.
        editor.update(cx, |_, cx| cx.notify());
        let t = Instant::now();
        cx.simulate_keystrokes("secondary-a");
        let select_all_key = t.elapsed();
        let t = Instant::now();
        draw(cx);
        let select_all_frame = t.elapsed();
        let next_frame = frame(cx);
        let t = Instant::now();
        cx.simulate_keystrokes("secondary-c");
        let copy = t.elapsed();
        let t = Instant::now();
        let json = editor.update(cx, |e, cx| e.document(cx)).to_json();
        let to_json = t.elapsed();
        let t = Instant::now();
        let markdown = editor.update(cx, |e, cx| e.markdown(cx));
        let to_markdown = t.elapsed();
        // Find a word on every line, then step.
        let t = Instant::now();
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| s.find("the", false, cx))
        });
        draw(cx);
        let find = t.elapsed();
        let matches = editor.update(cx, |e, cx| e.state().read(cx).find_matches().0.len());
        // Undo a delete of everything.
        cx.simulate_keystrokes("backspace");
        draw(cx);
        let t = Instant::now();
        cx.simulate_keystrokes("secondary-z");
        draw(cx);
        let undo_all = t.elapsed();
        eprintln!(
            "long page ({leaves} lines, {} KB Markdown, {} KB JSON, {} build):\n  open + first frame {open_time:?} (parse {parse:?}, editor made {made:?})\n  steady frame {steady:?}\n  keystroke + frame {typing:?}\n  enter + frame {enter:?}\n  scroll frame {scroll:?} median, {scroll_worst:?} worst, {scrolled} frames\n  select all {select_all_key:?} + frame {select_all_frame:?}, next frame {next_frame:?}\n  copy {copy:?}\n  document() to JSON {to_json:?}\n  markdown() {to_markdown:?}\n  find \"the\" ({matches} matches) + frame {find:?}\n  undo deleting everything + frame {undo_all:?}",
            markdown.len() / 1024,
            json.len() / 1024,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        );
    }

    /// A comment's quote, tinted from its anchor, follows its words as the text around them is
    /// edited, and goes when they do; a quote over two lines tints both.
    #[gpui::test]
    fn quoted_words_stay_tinted_as_the_text_changes(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "intro\n\n- an **item** here\n\nlast line");
        let leaf = |editor: &Entity<Editor>, cx: &mut VisualTestContext, ix: usize| {
            editor.update(cx, |e, cx| {
                e.state()
                    .read(cx)
                    .model()
                    .document()
                    .leaves()
                    .nth(ix)
                    .unwrap()
                    .id()
            })
        };
        let item = leaf(&editor, cx, 1);
        let select = |editor: &Entity<Editor>,
                      cx: &mut VisualTestContext,
                      from: (BlockId, usize),
                      to: (BlockId, usize)| {
            editor.update(cx, |e, cx| {
                e.state().update(cx, |s, cx| {
                    s.set_selection(
                        BlockSelection::Text {
                            anchor: BlockPosition::new(from.0, from.1),
                            head: BlockPosition::new(to.0, to.1),
                        },
                        cx,
                    )
                    .unwrap();
                })
            });
        };
        select(&editor, cx, (item, 3), (item, 12));
        let anchor = editor.update(cx, |e, cx| e.selection_anchor(cx)).unwrap();
        assert_eq!(anchor.1, "item here");
        editor.update(cx, |e, cx| e.highlight_quotes(vec![anchor.clone()], cx));
        draw(cx);
        let tints = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| e.state().read(cx).tints().to_vec())
        };
        assert_eq!(tints(&editor, cx), [(item, 3..12)]);
        // Typing before the words moves the tint with them.
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                s.set_selection(BlockSelection::caret(BlockPosition::new(item, 0)), cx)
                    .unwrap();
            })
        });
        cx.simulate_input("so ");
        draw(cx);
        assert_eq!(tints(&editor, cx), [(item, 6..15)]);
        // Deleting them takes the tint away.
        select(&editor, cx, (item, 6), (item, 15));
        cx.simulate_keystrokes("backspace");
        draw(cx);
        assert_eq!(tints(&editor, cx), []);
        // A quote over two paragraphs tints its line in each.
        let (intro, last) = (leaf(&editor, cx, 0), leaf(&editor, cx, 2));
        select(&editor, cx, (intro, 2), (last, 4));
        let anchor = editor.update(cx, |e, cx| e.selection_anchor(cx)).unwrap();
        editor.update(cx, |e, cx| e.highlight_quotes(vec![anchor], cx));
        let tinted = tints(&editor, cx);
        assert_eq!(tinted.first(), Some(&(intro, 2..5)));
        assert_eq!(tinted.last(), Some(&(last, 0..4)));
        editor.update(cx, |e, cx| e.highlight_quotes(Vec::new(), cx));
        assert_eq!(tints(&editor, cx), []);
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

    /// One embed's view changes by its lasting id, leaving another embed of the same database
    /// showing the same view as it is; the block keeps its lasting id, and undo takes it back.
    #[gpui::test]
    fn one_embeds_view_changes_by_its_block(cx: &mut TestAppContext) {
        let source = format!(
            "{}\n\n{}",
            crate::docs::embed("db1", "Tasks", "table", 300),
            crate::docs::embed("db1", "Tasks", "table", 200)
        );
        let (editor, cx) = open(cx, &source);
        let doc = editor.update(cx, |e, cx| e.document(cx));
        let second = doc.blocks[1].id.clone();
        assert!(editor.update(cx, |e, cx| e.set_embed_view_of(&second, "view-42_B", cx)));
        let want = format!(
            "{}\n\n{}",
            crate::docs::embed("db1", "Tasks", "table", 300),
            crate::docs::embed("db1", "Tasks", "view-42_B", 200)
        );
        assert_eq!(editor.update(cx, |e, cx| e.markdown(cx)), want);
        let after = editor.update(cx, |e, cx| e.document(cx));
        assert_eq!(after.blocks[1].id, second, "the block keeps its lasting id");
        assert_eq!(after.blocks[0].id, doc.blocks[0].id);
        // The view it already shows, an unknown id and a non-embed change nothing.
        assert!(!editor.update(cx, |e, cx| e.set_embed_view_of(&second, "view-42_B", cx)));
        assert!(!editor.update(cx, |e, cx| e.set_embed_view_of("nope", "board", cx)));
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
        // Rewriting every embed of the database by its view keeps their ids too.
        let n = editor.update(cx, |e, cx| e.set_embed_view("db1", "table", "board", cx));
        assert_eq!(n, 2);
        let after = editor.update(cx, |e, cx| e.document(cx));
        let ids: Vec<_> = after.blocks.iter().map(|b| b.id.clone()).collect();
        let before: Vec<_> = doc.blocks.iter().map(|b| b.id.clone()).collect();
        assert_eq!(ids, before);
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
        assert_eq!(left, gpui::px(COMPACT_SIDE), "compact: 24 in");
        assert!(top < gpui::px(COMPACT_INSET * 2.), "compact: {top:?} down");
        assert!(!hovered, "compact: no side menu");
    }

    /// Columns sit side by side in a wide editor, and one under another, in order, on a phone.
    #[gpui::test]
    fn columns_sit_side_by_side_and_stack_on_a_phone(cx: &mut TestAppContext) {
        let (editor, cx) = open(
            cx,
            "<!-- columns -->\n\n<!-- column 0.5 -->\n\nleft\n\n<!-- column 0.5 -->\n\nright\n\n<!-- /columns -->",
        );
        // Where each column's first line is drawn.
        let places = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            draw(cx);
            draw(cx);
            editor.update(cx, |e, cx| {
                let state = e.state().read(cx);
                let leaves: Vec<_> = state.model().document().leaves().map(|l| l.id()).collect();
                let at = |ix: usize| {
                    state
                        .position_bounds(BlockPosition::new(leaves[ix], 0))
                        .unwrap()
                };
                (at(0), at(1))
            })
        };
        cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
        let (left, right) = places(&editor, cx);
        assert!(
            (left.top() - right.top()).abs() < gpui::px(2.),
            "{left:?} {right:?}"
        );
        assert!(
            right.left() > left.left() + gpui::px(200.),
            "{left:?} {right:?}"
        );
        cx.simulate_resize(gpui::size(gpui::px(390.), gpui::px(800.)));
        let (left, right) = places(&editor, cx);
        assert!(right.top() > left.top(), "stacked: {left:?} {right:?}");
        assert!(
            (right.left() - left.left()).abs() < gpui::px(2.),
            "{left:?} {right:?}"
        );
    }

    /// In a host app's page column (605 wide), rows of three, four and five columns fit side by
    /// side, each its share; on a phone every column of a row stacks, each the whole width.
    #[gpui::test]
    fn columns_shrink_to_fit_and_stack_whole_on_a_phone(cx: &mut TestAppContext) {
        let row = |shares: &[&str]| {
            let mut s = String::from("<!-- columns -->");
            for (ix, share) in shares.iter().enumerate() {
                s.push_str(&format!(
                    "\n\n<!-- column {share} -->\n\nColumn {ix} with a few words in it."
                ));
            }
            s + "\n\n<!-- /columns -->"
        };
        let columns = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            draw(cx);
            draw(cx);
            editor.update(cx, |e, cx| {
                let state = e.state().read(cx);
                let row = state.top_level_blocks()[0].0;
                state
                    .children_bounds(row)
                    .into_iter()
                    .map(|(_, b)| b)
                    .collect::<Vec<_>>()
            })
        };
        for shares in [&["0.4", "0.3", "0.3"][..], &["0.25"; 4], &["0.2"; 5]] {
            let (editor, cx) = open(cx, &row(shares));
            // 605 of text: 733 less the two 64 gutters.
            cx.simulate_resize(gpui::size(gpui::px(733.), gpui::px(800.)));
            let cols = columns(&editor, cx);
            assert_eq!(cols.len(), shares.len());
            assert!(
                cols.windows(2)
                    .all(|w| (w[0].top() - w[1].top()).abs() < gpui::px(2.)
                        && w[1].left() > w[0].right()),
                "side by side: {cols:?}"
            );
            if shares.len() == 3 {
                // 40/30/30: the first is the widest, by its share.
                let (a, b) = (cols[0].size.width, cols[1].size.width);
                assert!(a > b && (a / b - 4. / 3.).abs() < 0.1, "{a:?} {b:?}");
            }
        }
        // A phone: all stacked, every one the whole width.
        let (editor, cx) = open(cx, &row(&["0.4", "0.3", "0.3"]));
        cx.simulate_resize(gpui::size(gpui::px(390.), gpui::px(800.)));
        let cols = columns(&editor, cx);
        let width = cols[0].size.width;
        assert!(
            cols.windows(2)
                .all(|w| w[1].top() > w[0].bottom() - gpui::px(1.)),
            "stacked: {cols:?}"
        );
        assert!(
            cols.iter()
                .all(|c| (c.size.width - width).abs() < gpui::px(1.)),
            "each the same, whole width: {cols:?}"
        );
        assert!(width > gpui::px(300.), "{width:?}");
    }

    /// A handle in the gap between two columns moves the line between them: letting go gives
    /// them their new widths, as one undo step.
    #[gpui::test]
    fn a_columns_handle_moves_the_line_between_them(cx: &mut TestAppContext) {
        let (editor, cx) = open(
            cx,
            "<!-- columns -->\n\n<!-- column 0.5 -->\n\nleft\n\n<!-- column 0.5 -->\n\nright\n\n<!-- /columns -->",
        );
        cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
        draw(cx);
        draw(cx);
        let block = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks()[0].1);
        cx.simulate_mouse_move(block.center(), None, gpui::Modifiers::none());
        draw(cx);
        let handle = cx
            .debug_bounds("blocks-column-handle-0")
            .expect("a handle shows in the gap")
            .center();
        cx.simulate_mouse_down(handle, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
        let to = gpui::point(handle.x - gpui::px(120.), handle.y);
        cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
        draw(cx);
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
        let widths = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| {
                let state = e.state().read(cx);
                let doc = state.model().document();
                let columns = doc.root().children().next().unwrap();
                columns
                    .children()
                    .map(|c| c.column_width().unwrap())
                    .collect::<Vec<f32>>()
            })
        };
        let w = widths(&editor, cx);
        assert!(
            w[0] < 0.45 && w[1] > 0.55 && (w[0] + w[1] - 1.).abs() < 0.01,
            "{w:?}"
        );
        assert!(
            markdown(&editor, cx).contains("<!-- column 0.3"),
            "{}",
            markdown(&editor, cx)
        );
        cx.simulate_keystrokes("secondary-z");
        draw(cx);
        assert_eq!(widths(&editor, cx), [0.5, 0.5], "one undo step");
    }

    /// A block in a column has its grip in the gap left of its column; dragging it out above
    /// the page's first block moves it there, and the row left with one column unwraps.
    #[gpui::test]
    fn a_block_in_a_column_has_a_grip_that_drags_it_out(cx: &mut TestAppContext) {
        let (editor, cx) = open(
            cx,
            "top\n\n<!-- columns -->\n\n<!-- column 0.5 -->\n\nleft\n\n<!-- column 0.5 -->\n\nright\n\n<!-- /columns -->",
        );
        cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
        draw(cx);
        draw(cx);
        let (top, right_column, right) = editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            let blocks = state.top_level_blocks();
            let columns = blocks[1].0;
            let cols = state.children_bounds(columns);
            let right = state.children_bounds(cols[1].0)[0].1;
            (blocks[0].1, cols[1].1, right)
        });
        cx.simulate_mouse_move(right.center(), None, gpui::Modifiers::none());
        draw(cx);
        let grip = cx
            .debug_bounds("blocks-side-grip")
            .expect("the block's grip shows")
            .center();
        assert!(
            grip.x < right_column.left() && grip.x > right_column.left() - gpui::px(24.),
            "in the gap left of its column: {grip:?} {right_column:?}"
        );
        cx.simulate_mouse_down(grip, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
        let to = gpui::point(top.center().x, top.top() - gpui::px(2.));
        cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
        draw(cx);
        assert!(
            editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()),
            "dragging"
        );
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(markdown(&editor, cx), "right\n\ntop\n\nleft");
        // The grip's release ended a drag, not a click: no block menu.
        assert_eq!(editor.read_with(cx, |e, _| e.block_menu), None);
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

    /// Stepping to a match out of view brings it to the middle of the page, as a browser's
    /// find does, not to the page's bottom edge; a match in view stays where it is.
    #[gpui::test]
    fn a_match_out_of_view_is_shown_in_the_middle(cx: &mut TestAppContext) {
        let source = (1..=200)
            .map(|n| match n {
                3 | 150 => format!("Line {n} has the needle"),
                _ => format!("Line {n}"),
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let (editor, cx) = open(cx, &source);
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-f"
        } else {
            "ctrl-f"
        });
        draw(cx);
        cx.simulate_input("needle");
        for _ in 0..8 {
            draw(cx);
        }
        let top = editor.read_with(cx, |e, _| e.scroll.offset().y);
        assert_eq!(top, gpui::px(0.), "the first match is in view: no scroll");
        editor.update(cx, |e, cx| e.step_find(true, cx));
        for _ in 0..8 {
            draw(cx);
        }
        let (found, view) = editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            let (matches, current) = state.find_matches();
            let m = &matches[current.unwrap()];
            (
                state
                    .position_bounds(BlockPosition::new(m.block, m.range.start))
                    .unwrap(),
                e.scroll.bounds(),
            )
        });
        let third = view.size.height / 3.;
        assert!(
            found.top() > view.top() + third && found.bottom() < view.bottom() - third,
            "in the middle: {found:?} in {view:?}"
        );
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

    /// A callout's block menu sets its icon and colour, and draws with them.
    #[gpui::test]
    fn the_block_menu_sets_a_callouts_icon_and_colour(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "> [!NOTE]\n> Read me");
        let callout = editor.update(cx, |e, cx| {
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
        editor.update(cx, |e, cx| {
            e.grip_pressed(callout);
            e.grip_released(cx);
        });
        draw(cx);
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(callout, "callout-icon-0", window, cx)
        });
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(callout, "callout-color-yellow", window, cx)
        });
        draw(cx);
        assert_eq!(
            editor.update(cx, |e, cx| e.markdown(cx)),
            "> [!NOTE] <!-- icon:💡 color:yellow -->\n> Read me"
        );
        // Not one of the menu's: nothing changes.
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(callout, "callout-color-teal", window, cx)
        });
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(callout, "callout-icon-none", window, cx)
        });
        assert_eq!(
            editor.update(cx, |e, cx| e.markdown(cx)),
            "> [!NOTE] <!-- color:yellow -->\n> Read me"
        );
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

    /// With blocks selected, a selected block's menu acts on all of them, as Notion's does:
    /// Turn into turns each, Duplicate and Delete take them all. Another block's menu acts on
    /// its own block. The format bar's Turn into turns every block the text runs through.
    #[gpui::test]
    fn the_block_menu_acts_on_every_selected_block(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "a\n\nb\n\nc");
        let ids = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| {
                e.state()
                    .read(cx)
                    .model()
                    .document()
                    .root()
                    .children()
                    .map(|b| b.id())
                    .collect::<Vec<markdown_editor::BlockId>>()
            })
        };
        let select = |editor: &Entity<Editor>, cx: &mut VisualTestContext, anchor, head| {
            editor.update(cx, |e, cx| {
                e.state().update(cx, |s, cx| {
                    s.set_selection(BlockSelection::Blocks { anchor, head }, cx)
                        .unwrap();
                })
            });
        };
        let pick = |editor: &Entity<Editor>, cx: &mut VisualTestContext, block, id: &str| {
            editor.update_in(cx, |e, window, cx| e.block_menu_pick(block, id, window, cx));
        };
        let at = ids(&editor, cx);
        select(&editor, cx, at[0], at[1]);
        // Heading 2 (the slash menu's third built-in block), from the second block's menu.
        pick(&editor, cx, at[1], "turn-2");
        assert_eq!(markdown(&editor, cx), "## a\n\n## b\n\nc");
        let at = ids(&editor, cx);
        select(&editor, cx, at[0], at[1]);
        pick(&editor, cx, at[0], "duplicate");
        assert_eq!(markdown(&editor, cx), "## a\n\n## b\n\n## a\n\n## b\n\nc");
        // The last block isn't selected: its menu acts on it alone.
        let at = ids(&editor, cx);
        select(&editor, cx, at[0], at[1]);
        pick(&editor, cx, at[4], "turn-1");
        assert_eq!(markdown(&editor, cx), "## a\n\n## b\n\n## a\n\n## b\n\n# c");
        let at = ids(&editor, cx);
        select(&editor, cx, at[2], at[3]);
        pick(&editor, cx, at[3], "delete");
        assert_eq!(markdown(&editor, cx), "## a\n\n## b\n\n# c");
        // The format bar's Turn into, on text from the first block to the third.
        let (first, last) = editor.update(cx, |e, cx| {
            let doc = e.state().read(cx).model().document();
            let leaves: Vec<_> = doc.leaves().map(|l| l.id()).collect();
            (leaves[0], leaves[2])
        });
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                s.set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(first, 0),
                        head: BlockPosition::new(last, 1),
                    },
                    cx,
                )
                .unwrap();
            })
        });
        let top = editor
            .update(cx, |e, cx| e.selection_top_block(cx))
            .unwrap();
        // Text (the first built-in block).
        pick(&editor, cx, top, "turn-0");
        assert_eq!(markdown(&editor, cx), "a\n\nb\n\nc");
    }

    /// The block menu's Color gives all of the block's text, or every selected block's, a text
    /// or background colour, as Notion colours a block; its Default takes it off.
    #[gpui::test]
    fn the_block_menu_colours_whole_blocks(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "one **two**\n\nthree\n\nfour");
        let ids = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| {
                e.state()
                    .read(cx)
                    .model()
                    .document()
                    .root()
                    .children()
                    .map(|b| b.id())
                    .collect::<Vec<markdown_editor::BlockId>>()
            })
        };
        let pick = |editor: &Entity<Editor>, cx: &mut VisualTestContext, block, id: &str| {
            editor.update_in(cx, |e, window, cx| e.block_menu_pick(block, id, window, cx));
            draw(cx);
        };
        let at = ids(&editor, cx);
        // Red text (the ninth), on the first block alone.
        pick(&editor, cx, at[0], "text-8");
        assert_eq!(
            markdown(&editor, cx),
            "<span style=\"color:#d44c47\">one **two**</span>\n\nthree\n\nfour"
        );
        // A blue background (the sixth) on the two selected blocks.
        let at = ids(&editor, cx);
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                s.set_selection(
                    BlockSelection::Blocks {
                        anchor: at[1],
                        head: at[2],
                    },
                    cx,
                )
                .unwrap();
            })
        });
        pick(&editor, cx, at[2], "background-5");
        assert_eq!(
            markdown(&editor, cx),
            "<span style=\"color:#d44c47\">one **two**</span>\n\n<mark color=\"#e7f3f8\">three</mark>\n\n<mark color=\"#e7f3f8\">four</mark>"
        );
        let at = ids(&editor, cx);
        pick(&editor, cx, at[0], "text-none");
        assert!(markdown(&editor, cx).starts_with("one **two**\n\n"));
    }

    /// A code block with the caret in it has its language, a menu of common ones, and Copy over
    /// its top right; another block doesn't; a read-only document has Copy alone.
    #[gpui::test]
    fn a_code_blocks_chrome_sets_its_language_and_copies_it(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "```rust\nfn main() {}\n```\n\ntext");
        caret_at_end(&editor, cx, 1);
        assert!(cx.debug_bounds("blocks-code-copy").is_none(), "not on text");
        caret_at_end(&editor, cx, 0);
        assert!(cx.debug_bounds("blocks-code-copy").is_some(), "on code");
        press(cx, "blocks-code-copy");
        let copied = cx.read_from_clipboard().and_then(|item| item.text());
        assert_eq!(copied.as_deref(), Some("fn main() {}"));
        press(cx, "blocks-code-language");
        let go = cx
            .debug_bounds("menu-item-lang-go")
            .expect("the language menu is open")
            .center();
        cx.simulate_click(go, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(markdown(&editor, cx), "```go\nfn main() {}\n```\n\ntext");
        let block = editor.update(cx, |e, cx| {
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
        editor.update_in(cx, |e, window, cx| {
            e.set_code_language(block, "", window, cx)
        });
        assert_eq!(markdown(&editor, cx), "```\nfn main() {}\n```\n\ntext");
        editor.update(cx, |e, cx| e.set_readonly(true, cx));
        draw(cx);
        assert!(cx.debug_bounds("blocks-code-copy").is_some());
        assert!(cx.debug_bounds("blocks-code-language").is_none());
    }

    /// A drag from the page's margin, beside the blocks, selects the blocks it sweeps over,
    /// whole, as Notion's does; a drag on the text still selects text.
    #[gpui::test]
    fn a_drag_from_the_margin_selects_the_blocks_it_sweeps(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "one\n\ntwo\n\nthree\n\nfour");
        cx.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
        draw(cx);
        draw(cx);
        let blocks = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks());
        let selection = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| e.state().read(cx).model().selection().clone())
        };
        let sweep = |cx: &mut VisualTestContext, from: gpui::Point<gpui::Pixels>, to| {
            cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
            cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
            cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
            draw(cx);
        };
        // In the right margin, past every block's text.
        let x = blocks
            .iter()
            .map(|(_, b)| b.right())
            .fold(gpui::px(0.), gpui::Pixels::max)
            + gpui::px(20.);
        let at = |ix: usize| gpui::point(x, blocks[ix].1.center().y);
        sweep(cx, at(1), at(3));
        assert_eq!(
            selection(&editor, cx),
            BlockSelection::Blocks {
                anchor: blocks[1].0,
                head: blocks[3].0
            }
        );
        sweep(cx, at(3), at(0));
        assert_eq!(
            selection(&editor, cx),
            BlockSelection::Blocks {
                anchor: blocks[3].0,
                head: blocks[0].0
            },
            "upward"
        );
        // On the text: text.
        let text =
            |ix: usize| gpui::point(blocks[ix].1.left() + gpui::px(2.), blocks[ix].1.center().y);
        sweep(cx, text(0), text(1));
        assert!(matches!(
            selection(&editor, cx),
            BlockSelection::Text { .. }
        ));
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

    /// Typing a colour's name in the slash menu colours the whole line, as Notion's does; the
    /// colours aren't listed until typed.
    #[gpui::test]
    fn the_slash_menu_colours_the_line(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "Plan the launch");
        caret_at_end(&editor, cx, 0);
        cx.simulate_input(" /");
        draw(cx);
        assert!(
            !menu(&editor, cx)
                .unwrap()
                .iter()
                .any(|t| t.contains("background")),
            "colours aren't listed until typed"
        );
        cx.simulate_input("red");
        draw(cx);
        assert_eq!(menu(&editor, cx).unwrap()[0], "Red");
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            markdown(&editor, cx)
                .starts_with("<span style=\"color:#d44c47\">Plan the launch</span>"),
            "{}",
            markdown(&editor, cx)
        );
        cx.simulate_input(" /blue backg");
        draw(cx);
        assert_eq!(
            menu(&editor, cx).map(|m| m.first().cloned()),
            Some(Some("Blue background".to_string())),
            "{}",
            editor.update(cx, |e, cx| format!(
                "{:?}",
                e.state().read(cx).caret_prefix()
            ))
        );
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            markdown(&editor, cx).starts_with("<mark color=\"#e7f3f8\">"),
            "{}",
            markdown(&editor, cx)
        );
        cx.simulate_input("/default text");
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            !markdown(&editor, cx).contains("<span"),
            "{}",
            markdown(&editor, cx)
        );
    }

    /// The slash menu's Toggle heading makes the line a heading that folds, as one undo step.
    #[gpui::test]
    fn the_slash_menu_makes_a_toggle_heading(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("Plan");
        cx.simulate_input(" /toggle heading 2");
        draw(cx);
        assert_eq!(menu(&editor, cx).unwrap()[0], "Toggle heading 2");
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            markdown(&editor, cx).starts_with("<details open>\n<summary><h2>Plan"),
            "{}",
            markdown(&editor, cx)
        );
        cx.simulate_keystrokes("secondary-z");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "Plan /toggle heading 2");
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
            menu(&editor, cx).unwrap()[..3],
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

    /// A sentence after a slash closes the menu once nothing matches, and doesn't open it again
    /// as the words go on; Enter is then Enter.
    #[gpui::test]
    fn words_after_a_slash_close_the_menu_and_enter_is_enter(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "");
        cx.simulate_input("and/or we could");
        draw(cx);
        assert_eq!(menu(&editor, cx), None);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "and/or we could\n\n");
        // At a line's start: open while it matches, then closed for good.
        cx.simulate_input("/heading");
        draw(cx);
        assert!(menu(&editor, cx).is_some());
        for c in " is fine, then heading 1".chars() {
            cx.simulate_input(&c.to_string());
            draw(cx);
        }
        assert_eq!(menu(&editor, cx), None, "never back once closed");
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            markdown(&editor, cx).contains("/heading is fine, then heading 1\n\n"),
            "{}",
            markdown(&editor, cx)
        );
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
        cx.simulate_keystrokes("secondary-shift-down");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "two\n\none\n\n- a\n- b");
        caret_at_end(&editor, cx, 3);
        cx.simulate_keystrokes("secondary-shift-up");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "two\n\none\n\n- b\n- a");
        cx.simulate_keystrokes("secondary-z secondary-z");
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
        cx.simulate_keystrokes("secondary-shift-down");
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

    /// The touch menu goes above the selection, as iOS's, and below it when the selection is
    /// at the top of the page with no room above.
    #[gpui::test]
    fn the_touch_menu_goes_below_a_selection_at_the_top(cx: &mut TestAppContext) {
        let source = (1..=12)
            .map(|n| format!("Line number {n}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let (editor, cx) = open(cx, &source);
        cx.simulate_resize(gpui::size(gpui::px(402.), gpui::px(874.)));
        draw(cx);
        draw(cx);
        let at = |editor: &Entity<Editor>, cx: &mut VisualTestContext, leaf: usize| {
            editor.update(cx, |e, cx| {
                let state = e.state().read(cx);
                let id = state.model().document().leaves().nth(leaf).unwrap().id();
                state.position_bounds(BlockPosition::new(id, 2)).unwrap()
            })
        };
        // The first line, at the top of a phone's page: no room above.
        let first = at(&editor, cx, 0);
        long_press(cx, first.center());
        draw(cx);
        let menu = cx.debug_bounds("blocks-edit-menu").expect("the menu");
        assert!(
            menu.top() >= first.bottom(),
            "below: {menu:?} under {first:?}"
        );
        // A line well down the page: above it, as iOS's.
        let lower = at(&editor, cx, 6);
        assert!(lower.top() > gpui::px(200.), "{lower:?}");
        long_press(cx, lower.center());
        draw(cx);
        let menu = cx.debug_bounds("blocks-edit-menu").expect("the menu");
        assert!(
            menu.bottom() <= lower.top(),
            "above: {menu:?} over {lower:?}"
        );
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
        assert!(
            cx.debug_bounds("blocks-format-comment").is_none(),
            "no Comment unless the app takes comments"
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

    /// With [`Options::comments`], the bar's Comment asks the app for a thread on the
    /// selection, with its anchor; without it the bar has no Comment.
    #[gpui::test]
    fn the_format_bar_asks_for_a_comment(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(|window, cx| {
            Editor::with_options(
                "intro\n\nhello world",
                super::Options::default().comments(),
                window,
                cx,
            )
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        let asked = std::rc::Rc::new(std::cell::RefCell::new(None));
        let seen = asked.clone();
        cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &super::EditorEvent, _| {
                if let super::EditorEvent::CommentRequested { block, quote } = event {
                    *seen.borrow_mut() = Some((block.clone(), quote.clone()));
                }
            })
            .detach()
        });
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let leaf = s.model().document().leaves().nth(1).unwrap().id();
                s.set_selection(
                    BlockSelection::Text {
                        anchor: BlockPosition::new(leaf, 6),
                        head: BlockPosition::new(leaf, 11),
                    },
                    cx,
                )
                .unwrap();
            })
        });
        draw(cx);
        press(cx, "blocks-format-comment");
        let lasting = editor.update(cx, |e, cx| e.document(cx).blocks[1].id.clone());
        assert_eq!(
            asked.borrow().clone(),
            Some((lasting, "world".to_string())),
            "the app gets the selection's anchor"
        );
        assert_eq!(markdown(&editor, cx), "intro\n\nhello world");
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

    /// Color gives the selection a text colour, stored as `<span style="color">`, or a
    /// background colour, stored as `<mark color>`, shows them checked, and takes them off
    /// again; ⌘⇧H gives the last colour again, or takes it off.
    #[gpui::test]
    fn the_format_bar_colours_the_selection(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world again");
        drag_select(&editor, cx, 6, 11);
        press(cx, "blocks-format-color");
        assert!(
            cx.debug_bounds("menu-item-text-none").is_some(),
            "the menu is open"
        );
        // The menu scrolls to its backgrounds: pick as its rows do. Blue: each section's
        // entries are Default, then Gray, Brown, Orange, Yellow, Green and Blue.
        let pick = |editor: &Entity<Editor>, cx: &mut VisualTestContext, id: &str| {
            editor.update_in(cx, |e, window, cx| e.color_menu_pick(id, window, cx));
            draw(cx);
        };
        pick(&editor, cx, "background-5");
        assert_eq!(
            markdown(&editor, cx),
            "hello <mark color=\"#e7f3f8\">world</mark> again"
        );
        let paints = |editor: &Entity<Editor>, cx: &mut VisualTestContext| {
            editor.update(cx, |e, cx| {
                let (text, background) = e.selection_paints(cx);
                (text.map(|c| c.name), background.map(|c| c.name))
            })
        };
        assert_eq!(
            paints(&editor, cx),
            (None, Some("Blue")),
            "the selection stays"
        );
        // Red text, over the blue.
        press(cx, "blocks-format-color");
        press(cx, "menu-item-text-8");
        assert_eq!(
            markdown(&editor, cx),
            "hello <mark color=\"#e7f3f8\"><span style=\"color:#d44c47\">world</span></mark> again"
        );
        assert_eq!(paints(&editor, cx), (Some("Red"), Some("Blue")));
        pick(&editor, cx, "background-none");
        assert_eq!(
            markdown(&editor, cx),
            "hello <span style=\"color:#d44c47\">world</span> again"
        );
        press(cx, "blocks-format-color");
        press(cx, "menu-item-text-none");
        assert_eq!(markdown(&editor, cx), "hello world again");
        // ⌘⇧H: the last colour picked (red text), then off.
        let key = "secondary-shift-h";
        cx.simulate_keystrokes(key);
        assert_eq!(
            markdown(&editor, cx),
            "hello <span style=\"color:#d44c47\">world</span> again"
        );
        cx.simulate_keystrokes(key);
        assert_eq!(markdown(&editor, cx), "hello world again");
    }

    /// On touch, the edit menu's Color opens the colours in a sheet; a pick colours the
    /// selection and closes it.
    #[gpui::test]
    fn touch_colours_the_selection_from_a_sheet(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "hello world again");
        let at = point_at(&editor, cx, 8);
        long_press(cx, at);
        tap(cx, "blocks-touch-color");
        assert!(
            editor.read_with(cx, |e, _| e.color_sheet),
            "the sheet opens"
        );
        assert!(
            cx.debug_bounds("blocks-edit-menu").is_none(),
            "the menu goes"
        );
        let red = cx
            .debug_bounds("menu-item-text-8")
            .expect("the text colours are in the sheet")
            .center();
        cx.simulate_click(red, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "hello <span style=\"color:#d44c47\">world</span> again"
        );
        assert!(
            !editor.read_with(cx, |e, _| e.color_sheet),
            "picking closes it"
        );
    }

    /// On touch, a long press on the text then a move extends the word selection, as iOS does;
    /// one begun off the text, in the gutter left of it, lifts the block under the finger and
    /// drags it instead, and letting go drops it, as one undo step. A long press let go without
    /// moving still selects the word and shows the edit menu.
    #[gpui::test]
    fn a_long_press_in_the_gutter_and_a_move_drag_a_block_by_touch(cx: &mut TestAppContext) {
        use gpui::TouchPhase::*;
        let (editor, cx) = open(cx, "first words\n\nsecond\n\nthird");
        let blocks = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks());
        let long_press_then =
            |cx: &mut VisualTestContext, at, moves: &[gpui::Point<gpui::Pixels>]| {
                touch(cx, Started, at);
                cx.executor()
                    .advance_clock(std::time::Duration::from_secs(1));
                draw(cx);
                for to in moves {
                    touch(cx, Moved, *to);
                }
            };
        // On the text: the word, and moving extends the selection.
        let word = point_at(&editor, cx, 8);
        let back = point_at(&editor, cx, 2);
        long_press_then(cx, word, &[back]);
        assert!(!editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()));
        let selection = editor.update(cx, |e, cx| e.state().read(cx).model().selection().clone());
        assert!(
            matches!(selection, BlockSelection::Text { anchor, head } if anchor.offset == 6 && head.offset == 2),
            "extended: {selection:?}"
        );
        touch(cx, Ended, back);
        // The caret down again, and the selection's handles gone.
        caret_at_end(&editor, cx, 2);
        assert!(cx.debug_bounds("blocks-handle-start").is_none());
        // In the gutter: within reach of the press nothing lifts; past it the block does.
        let gutter = gpui::point(blocks[0].1.left() - gpui::px(12.), blocks[0].1.center().y);
        long_press_then(
            cx,
            gutter,
            &[gpui::point(gutter.x, gutter.y + gpui::px(4.))],
        );
        assert!(!editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()));
        let to = gpui::point(gutter.x, blocks[2].1.center().y + gpui::px(4.));
        touch(cx, Moved, to);
        assert!(
            editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()),
            "lifted"
        );
        assert!(
            cx.debug_bounds("blocks-edit-menu").is_none(),
            "no edit menu while dragging"
        );
        touch(cx, Ended, to);
        assert!(!editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()));
        assert_eq!(markdown(&editor, cx), "second\n\nthird\n\nfirst words");
        cx.simulate_keystrokes("secondary-z");
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "first words\n\nsecond\n\nthird",
            "one undo step"
        );
        // A long press let go where it was: the word and the edit menu, as before.
        let blocks = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks());
        long_press(cx, blocks[1].1.center());
        assert!(cx.debug_bounds("blocks-edit-menu").is_some());
    }

    /// The block sheet's Move lifts the block where it is, and the next finger drag moves it.
    #[gpui::test]
    fn the_block_sheets_move_lifts_the_block_for_the_next_drag(cx: &mut TestAppContext) {
        use gpui::TouchPhase::*;
        let (editor, cx) = open(cx, "first\n\nsecond\n\nthird");
        let blocks = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks());
        long_press(cx, blocks[0].1.center());
        tap(cx, "blocks-menu-Block");
        draw(cx);
        let move_item = cx
            .debug_bounds("menu-item-move")
            .expect("the sheet has Move")
            .center();
        cx.simulate_click(move_item, gpui::Modifiers::none());
        draw(cx);
        assert!(
            editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()),
            "lifted in place"
        );
        // A finger drag anywhere moves it by as much as the finger moves.
        let from = gpui::point(blocks[1].1.center().x, blocks[1].1.center().y);
        let by = blocks[2].1.center().y - blocks[0].1.center().y + gpui::px(4.);
        touch(cx, Started, from);
        touch(cx, Moved, gpui::point(from.x, from.y + by / 2.));
        touch(cx, Moved, gpui::point(from.x, from.y + by));
        touch(cx, Ended, gpui::point(from.x, from.y + by));
        assert!(!editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()));
        assert_eq!(markdown(&editor, cx), "second\n\nthird\n\nfirst");
    }

    /// A tap on a block lifted by the sheet's Move sets it down where it was.
    #[gpui::test]
    fn a_tap_sets_a_lifted_block_down_where_it_was(cx: &mut TestAppContext) {
        use gpui::TouchPhase::*;
        let (editor, cx) = open(cx, "first\n\nsecond\n\nthird");
        let blocks = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks());
        long_press(cx, blocks[0].1.center());
        tap(cx, "blocks-menu-Block");
        draw(cx);
        let move_item = cx
            .debug_bounds("menu-item-move")
            .expect("the sheet has Move")
            .center();
        cx.simulate_click(move_item, gpui::Modifiers::none());
        draw(cx);
        assert!(editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()));
        let at = blocks[2].1.center();
        touch(cx, Started, at);
        touch(cx, Ended, at);
        draw(cx);
        assert!(
            !editor.update(cx, |e, cx| e.state().read(cx).is_dragging_block()),
            "set down"
        );
        assert_eq!(
            markdown(&editor, cx),
            "first\n\nsecond\n\nthird",
            "where it was"
        );
    }

    /// While blocks are dragged near the page's bottom edge, the page scrolls.
    #[gpui::test]
    fn dragging_a_block_near_the_edge_scrolls_the_page(cx: &mut TestAppContext) {
        let text: Vec<String> = (0..60).map(|i| format!("line {i}")).collect();
        let (editor, cx) = open(cx, &text.join("\n\n"));
        cx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(400.)));
        draw(cx);
        draw(cx);
        let (block, b) = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks()[1]);
        let view = editor.read_with(cx, |e, _| e.scroll.bounds());
        let from = gpui::point(b.left() - gpui::px(20.), b.center().y);
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, _| s.begin_block_drag(block, from))
        });
        let near = gpui::point(from.x, view.bottom() - gpui::px(10.));
        cx.simulate_mouse_move(near, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
        for _ in 0..10 {
            draw(cx);
        }
        let offset = editor.read_with(cx, |e, _| e.scroll.offset().y);
        assert!(offset < gpui::px(0.), "scrolled down: {offset:?}");
        cx.simulate_mouse_up(near, gpui::MouseButton::Left, gpui::Modifiers::none());
        draw(cx);
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

    /// An editor whose app reads the clipboard's HTML from `html`.
    fn open_with_html<'a>(
        cx: &'a mut TestAppContext,
        source: &str,
        html: std::rc::Rc<std::cell::RefCell<Option<String>>>,
    ) -> (Entity<Editor>, &'a mut VisualTestContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let source = source.to_string();
        let (editor, cx) = cx.add_window_view(move |window, cx| {
            let options = super::Options::default().html_clipboard(move |_| html.borrow().clone());
            Editor::with_options(&source, options, window, cx)
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        (editor, cx)
    }

    /// With [`Options::block_link`], the block menu's Copy link to block copies the app's link
    /// for the block's lasting id, and that id reveals the block; without it the menu has none.
    #[gpui::test]
    fn copy_link_to_block_copies_the_apps_link(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let options = super::Options::default().block_link(|id| format!("app://doc/page#{id}"));
            Editor::with_options("first\n\nsecond", options, window, cx)
        });
        draw(cx);
        let second = editor.update(cx, |e, cx| {
            e.state()
                .read(cx)
                .model()
                .document()
                .root()
                .children()
                .nth(1)
                .unwrap()
                .id()
        });
        editor.update(cx, |e, cx| {
            e.grip_pressed(second);
            e.grip_released(cx);
        });
        draw(cx);
        assert!(
            cx.debug_bounds("menu-item-copy-link").is_some(),
            "the menu has it"
        );
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(second, "copy-link", window, cx)
        });
        let copied = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .expect("a link is copied");
        let id = copied
            .strip_prefix("app://doc/page#")
            .expect("the app's link")
            .to_string();
        let lasting = editor.update(cx, |e, cx| {
            e.document(cx).blocks.get(1).map(|b| b.id.clone())
        });
        assert_eq!(
            lasting.as_deref(),
            Some(id.as_str()),
            "the block's lasting id"
        );
        assert_eq!(
            markdown(&editor, cx),
            "first\n\nsecond",
            "nothing else changes"
        );
        let revealed = editor.update_in(cx, |e, window, cx| e.reveal_block(&id, window, cx));
        assert!(revealed);
        // Without the option, no entry.
        let (editor, cx) = open(cx, "first");
        let block = editor.update(cx, |e, cx| {
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
        editor.update(cx, |e, cx| {
            e.grip_pressed(block);
            e.grip_released(cx);
        });
        draw(cx);
        assert!(cx.debug_bounds("menu-item-copy-link").is_none());
    }

    fn paste_key(cx: &mut VisualTestContext, plain: bool) {
        let key = match (cfg!(target_os = "macos"), plain) {
            (true, false) => "cmd-v",
            (true, true) => "cmd-shift-v",
            (false, false) => "ctrl-v",
            (false, true) => "ctrl-shift-v",
        };
        cx.simulate_keystrokes(key);
        draw(cx);
    }

    /// A paste with HTML on the clipboard keeps its structure and marks; without HTML, or with
    /// ⇧⌘V, it's the plain text as before.
    #[gpui::test]
    fn pasted_html_keeps_its_structure(cx: &mut TestAppContext) {
        let html = std::rc::Rc::new(std::cell::RefCell::new(None));
        let (editor, cx) = open_with_html(cx, "Notes", html.clone());
        caret_at_end(&editor, cx, 0);
        cx.simulate_keystrokes("enter");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            "Setup\nRun it\nOne\nTwo".into(),
        ));
        *html.borrow_mut() = Some(
            "<meta charset='utf-8'><h2>Setup</h2><p>Run <b>it</b> now</p><ul><li>One</li><li>Two</li></ul>"
                .into(),
        );
        paste_key(cx, false);
        assert_eq!(
            markdown(&editor, cx),
            "Notes\n\n## Setup\n\nRun **it** now\n\n- One\n- Two"
        );
        // One line of HTML goes in at the caret with its marks.
        cx.simulate_keystrokes("enter enter");
        cx.simulate_input("See ");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("the docs".into()));
        *html.borrow_mut() = Some(r#"<a href="https://example.com">the <i>docs</i></a>"#.into());
        paste_key(cx, false);
        assert!(
            markdown(&editor, cx).ends_with("See [the *docs*](https://example.com)"),
            "{}",
            markdown(&editor, cx)
        );
        // ⇧⌘V and a clipboard without HTML paste the plain text.
        cx.simulate_keystrokes("enter");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("**plain**".into()));
        *html.borrow_mut() = Some("<b>rich</b>".into());
        paste_key(cx, true);
        *html.borrow_mut() = None;
        paste_key(cx, false);
        let text = markdown(&editor, cx);
        assert!(text.ends_with(r"\*\*plain\*\*\*\*plain\*\*"), "{text}");
    }

    /// A read-only document takes no paste, HTML or not.
    #[gpui::test]
    fn a_read_only_document_takes_no_html_paste(cx: &mut TestAppContext) {
        let html = std::rc::Rc::new(std::cell::RefCell::new(Some("<h1>No</h1>".to_string())));
        let (editor, cx) = open_with_html(cx, "Kept", html);
        editor.update(cx, |e, cx| e.set_readonly(true, cx));
        caret_at_end(&editor, cx, 0);
        paste_key(cx, false);
        assert_eq!(markdown(&editor, cx), "Kept");
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
        cx.simulate_keystrokes("secondary-z");
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
        cx.simulate_keystrokes("secondary-shift-up");
        draw(cx);
        assert!(markdown(&editor, cx).contains("<summary>Title</summary>"));
    }

    #[gpui::test]
    fn cmd_shift_arrows_in_a_table_keep_its_columns(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "| h1 | h2 |\n| --- | --- |\n| a | b |");
        caret_at_end(&editor, cx, 3);
        cx.simulate_keystrokes("secondary-shift-up");
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
