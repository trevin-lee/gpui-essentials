//! Links to other documents: an inline mention, and a sub-page block.
//!
//! An app that keeps many documents (pages in a tree, say) links them by id. The editor keeps
//! the id and draws what the app says about it, its current title and icon, so a link never goes
//! stale when its document is renamed. In Markdown both are ordinary links whose destination is
//! `doc:` and the id, so any other tool shows them as links:
//!
//! ```markdown
//! See [Roadmap](doc:4f2a) for the dates.
//!
//! [Meeting notes](doc:9c1e "page")
//! ```
//!
//! - A **mention** is a `doc:` link inside text. It's one unit, like a picture: the caret steps
//!   over it, and Backspace deletes it whole.
//! - A **sub-page** is a line holding only a `doc:` link titled `page`. It's drawn as a block
//!   with the document's icon and title, and is selected, moved and deleted whole.
//!
//! The link's text is the title when it was written, shown only while the app has no title for
//! the id. A click on either sends [`EditorEvent::DocClicked`](crate::EditorEvent::DocClicked);
//! the editor never opens anything itself. An id is 1 to 128 of `A-Z a-z 0-9 - _ . ~`; a
//! `doc:` link with any other id stays a plain link.
//!
//! Give the editor the app's documents with [`Options::docs`](crate::Options::docs), which also
//! adds Page and Link to page to the slash menu.

use std::sync::Arc;

use gpui::*;
use gpui_core::ActiveTheme as _;
use markdown::mdast::{Link, Node};
use markdown_editor::{CustomNode, Extension, InlineContext};

/// The link destination's prefix that marks a link to a document.
pub const SCHEME: &str = "doc:";
/// The link title that makes a line holding only a document link a sub-page.
pub const PAGE_TITLE: &str = "page";
/// The longest id the editor reads as a document's.
const MAX_ID: usize = 128;
/// What's shown for a document with no title.
const UNTITLED: &str = "Untitled";

/// What the editor shows for a document. Make one with [`DocInfo::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocInfo {
    pub title: SharedString,
    /// Drawn before the title: an emoji or a short glyph. A page glyph when `None`.
    pub icon: Option<SharedString>,
}

impl DocInfo {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            icon: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// The app's documents, as far as the editor needs them.
pub trait Docs: Send + Sync + 'static {
    /// The document with this id, or `None` when there isn't one (deleted, or not shared with
    /// this person). Called each time a link is drawn; keep it cheap, from a cache the app keeps.
    fn info(&self, id: &str, cx: &App) -> Option<DocInfo>;

    /// The name of the view `view` of the document `doc`, for an embed's header: a database
    /// view's own name ("Launch tasks" for `v-board`). `None` when the app can't name it; the
    /// header then names only the built-in kinds of view (Board, Table…) and shows nothing for
    /// any other. Called each time an embed is drawn; keep it cheap.
    fn view_name(&self, doc: &str, view: &str, cx: &App) -> Option<SharedString> {
        let _ = (doc, view, cx);
        None
    }
}

/// The name an embed's header shows for `view`: the app's, else a built-in kind's, else none
/// (an id the app couldn't name is never shown).
pub(crate) fn view_label(
    docs: Option<&dyn Docs>,
    doc: &str,
    view: &str,
    cx: &App,
) -> Option<SharedString> {
    docs.and_then(|d| d.view_name(doc, view, cx))
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            Some(
                match view {
                    "table" => "Table",
                    "board" => "Board",
                    "mindmap" => "Mind map",
                    "calendar" => "Calendar",
                    "page" => "Page",
                    _ => return None,
                }
                .into(),
            )
        })
}

/// The id in a link destination, when it's a document link with a valid id.
pub fn doc_id(url: &str) -> Option<&str> {
    let id = url.strip_prefix(SCHEME)?;
    let valid = (1..=MAX_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'));
    valid.then_some(id)
}

/// The Markdown of a mention of the document `id`, with `title` as its text.
pub fn mention(id: &str, title: &str) -> String {
    format!("[{}]({SCHEME}{id})", link_text(title))
}

/// The Markdown of a sub-page block for the document `id`, with `title` as its text.
pub fn page(id: &str, title: &str) -> String {
    format!("[{}]({SCHEME}{id} \"{PAGE_TITLE}\")", link_text(title))
}

/// `title` as a link's text: on one line, with the characters Markdown would read as syntax
/// escaped.
pub(crate) fn link_text(title: &str) -> String {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = if title.is_empty() { UNTITLED } else { &title };
    let mut out = String::with_capacity(title.len());
    for c in title.chars() {
        if matches!(
            c,
            '\\' | '[' | ']' | '*' | '_' | '`' | '<' | '>' | '~' | '&' | '$' | '|'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The plain text of a link's children.
pub(crate) fn plain_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.value),
            Node::InlineCode(c) => out.push_str(&c.value),
            Node::Break(_) => out.push(' '),
            other => {
                if let Some(children) = other.children() {
                    out.push_str(&plain_text(children));
                }
            }
        }
    }
    out
}

/// A document link as the editor keeps it: the id, and the title it was written with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocRef {
    pub(crate) id: SharedString,
    pub(crate) title: SharedString,
}

impl DocRef {
    fn from_link(link: &Link) -> Option<Self> {
        let id = doc_id(&link.url)?;
        let title = plain_text(&link.children);
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        Some(Self {
            id: id.to_string().into(),
            title: if title.is_empty() {
                UNTITLED.into()
            } else {
                title.into()
            },
        })
    }
}

/// Called with a document's id when its link is clicked.
pub(crate) type OnDocClick = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;

/// What the links draw and do: the app's documents, and the editor to tell about clicks. Both
/// are empty for a document read without an editor.
#[derive(Clone, Default)]
pub(crate) struct DocHooks {
    pub(crate) docs: Option<Arc<dyn Docs>>,
    pub(crate) on_click: Option<OnDocClick>,
}

impl DocHooks {
    fn info(&self, doc: &DocRef, cx: &App) -> (SharedString, Option<SharedString>, bool) {
        match self.docs.as_ref().and_then(|d| d.info(&doc.id, cx)) {
            Some(info) => {
                let title = if info.title.trim().is_empty() {
                    UNTITLED.into()
                } else {
                    info.title
                };
                (title, info.icon, true)
            }
            None => (doc.title.clone(), None, false),
        }
    }

    fn click_handler(
        &self,
        id: SharedString,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static + use<> {
        let on_click = self.on_click.clone();
        move |_, window, cx| {
            if let Some(on_click) = &on_click {
                on_click(&id, window, cx);
            }
        }
    }
}

/// The glyph of a document with no icon of its own.
const PAGE_GLYPH: &str = "📄";

/// Inline document links: `[title](doc:id)` inside text.
pub(crate) struct MentionPlugin(pub(crate) DocHooks);

/// The custom node's name for a mention.
pub(crate) const MENTION: &str = "doc-mention";
/// The custom node's name for a sub-page.
pub(crate) const PAGE: &str = "doc-page";

impl Extension for MentionPlugin {
    fn name(&self) -> &str {
        MENTION
    }

    fn parse(&self, node: &Node) -> Option<CustomNode> {
        let Node::Link(link) = node else {
            return None;
        };
        let doc = DocRef::from_link(link)?;
        let label = format!("Link to {}", doc.title);
        // Copy as the title, written back as the link.
        let markdown = mention(&doc.id, &doc.title);
        Some(
            CustomNode::new(MENTION, doc.clone())
                .text(doc.title.clone())
                .accessibility_label(label)
                .markdown(markdown),
        )
    }

    fn render_inline(
        &self,
        node: &CustomNode,
        line_ctx: InlineContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, Pixels)> {
        let doc = node.data::<DocRef>()?;
        let (title, icon, found) = self.0.info(doc, cx);
        let theme = cx.theme();
        let accent = theme.accent;
        let size = line_ctx.font_size;
        let line = line_ctx.line_height;
        let chip = div()
            .debug_selector(|| format!("blocks-mention-{}", doc.id))
            .id(SharedString::from(format!("blocks-mention-{}", doc.id)))
            .role(Role::Link)
            .aria_label(title.clone())
            .h(line)
            .max_w(px(480.))
            .flex()
            .items_center()
            .gap(size * 0.2)
            .px(px(2.))
            .rounded(px(4.))
            .cursor_pointer()
            .hover(move |s| s.bg(accent))
            .text_size(size)
            .line_height(line)
            .text_color(if found {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .child(
                div()
                    .flex_none()
                    .text_size(size * 0.9)
                    .child(icon.unwrap_or_else(|| PAGE_GLYPH.into())),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_weight(FontWeight::MEDIUM)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(title),
            )
            // A press here is the link's, not the editor's caret.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(self.0.click_handler(doc.id.clone()));
        // The chip is one line tall and its text is centred in it, as the line's own text is.
        Some((chip.into_any_element(), (line + size * 0.7) / 2.))
    }
}

/// Sub-page blocks: a line holding only `[title](doc:id "page")`.
pub(crate) struct PagePlugin(pub(crate) DocHooks);

impl Extension for PagePlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        PAGE
    }

    fn parse(&self, node: &Node) -> Option<CustomNode> {
        let Node::Paragraph(paragraph) = node else {
            return None;
        };
        let [Node::Link(link)] = paragraph.children.as_slice() else {
            return None;
        };
        if link.title.as_deref() != Some(PAGE_TITLE) {
            return None;
        }
        let doc = DocRef::from_link(link)?;
        let label = format!("Page {}", doc.title);
        let markdown = page(&doc.id, &doc.title);
        Some(
            CustomNode::new(PAGE, doc.clone())
                .text(doc.title.clone())
                .accessibility_label(label)
                .markdown(markdown),
        )
    }

    fn render(&self, node: &CustomNode, _: &mut Window, cx: &mut App) -> AnyElement {
        let Some(doc) = node.data::<DocRef>() else {
            return div().into_any_element();
        };
        let (title, icon, found) = self.0.info(doc, cx);
        let theme = cx.theme();
        let accent = theme.accent;
        div()
            .debug_selector(|| format!("blocks-page-{}", doc.id))
            .id(SharedString::from(format!("blocks-page-{}", doc.id)))
            .role(Role::Link)
            .aria_label(format!("Page {title}"))
            .w_full()
            .my(px(2.))
            .px(px(4.))
            .py(px(3.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .hover(move |s| s.bg(accent))
            .text_color(if found {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .child(
                div()
                    .flex_none()
                    .text_size(px(16.))
                    .child(icon.unwrap_or_else(|| PAGE_GLYPH.into())),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_weight(FontWeight::MEDIUM)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(title),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(self.0.click_handler(doc.id.clone()))
            .into_any_element()
    }
}

/// The link title word that makes a line holding only a document link an embed.
pub const EMBED_TITLE: &str = "embed";
/// The view an embed without one shows.
pub const DEFAULT_VIEW: &str = "page";
/// The height an embed without one has, in points.
pub const DEFAULT_HEIGHT: u32 = 320;
/// The heights an embed can be dragged between, in points.
pub const HEIGHTS: std::ops::RangeInclusive<u32> = 80..=1600;

/// A document drawn inside another: which document, how the app shows it (a hint such as
/// `table`, `board`, `mindmap`, `calendar` or `page`), how tall it is, and which block it is.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Embed {
    pub doc: SharedString,
    pub view: SharedString,
    pub height: u32,
    /// The embed block's lasting id, as [`Editor::document`](crate::Editor::document) gives it,
    /// so two embeds of one document (even showing the same view) are told apart, and the app
    /// can keep what each shows. Empty when drawn without an editor.
    pub block: SharedString,
}

/// The Markdown of an embed of the document `id`: a line holding only a link, so other
/// Markdown tools show a link. `view` must be 1 to 64 of `A-Z a-z 0-9 - _` (a database view's
/// id fits), else it's `page`;
/// `height` is kept within [`HEIGHTS`].
pub fn embed(id: &str, title: &str, view: &str, height: u32) -> String {
    let view = if valid_view(view) { view } else { DEFAULT_VIEW };
    let height = height.clamp(*HEIGHTS.start(), *HEIGHTS.end());
    format!(
        "[{}]({SCHEME}{id} \"{EMBED_TITLE} {view} {height}\")",
        link_text(title)
    )
}

/// A mention's document and title read back from its Markdown.
pub(crate) fn mention_of(markdown: &str) -> Option<(String, String)> {
    let tree = markdown_editor::parse_markdown(markdown, &markdown::ParseOptions::gfm()).ok()?;
    let [Node::Paragraph(paragraph)] = tree.children()?.as_slice() else {
        return None;
    };
    let [Node::Link(link)] = paragraph.children.as_slice() else {
        return None;
    };
    let doc = DocRef::from_link(link)?;
    Some((doc.id.to_string(), doc.title.to_string()))
}

/// An embed's document, title, view and height read back from its Markdown.
pub(crate) fn embed_of(markdown: &str) -> Option<(String, String, String, u32)> {
    let tree = markdown_editor::parse_markdown(markdown, &markdown::ParseOptions::gfm()).ok()?;
    let [Node::Paragraph(paragraph)] = tree.children()?.as_slice() else {
        return None;
    };
    let [Node::Link(link)] = paragraph.children.as_slice() else {
        return None;
    };
    let (view, height) = embed_title(link.title.as_deref()?)?;
    let doc = DocRef::from_link(link)?;
    Some((doc.id.to_string(), doc.title.to_string(), view, height))
}

fn valid_view(view: &str) -> bool {
    (1..=64).contains(&view.len())
        && view
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// An embed's view and height from its link title (`embed`, `embed table`,
/// `embed table 320`), or `None` when the title isn't an embed's.
fn embed_title(title: &str) -> Option<(String, u32)> {
    let mut words = title.split_whitespace();
    if words.next()? != EMBED_TITLE {
        return None;
    }
    let view = words
        .next()
        .filter(|v| valid_view(v))
        .unwrap_or(DEFAULT_VIEW);
    let height = words
        .next()
        .and_then(|h| h.parse::<u32>().ok())
        .unwrap_or(DEFAULT_HEIGHT)
        .clamp(*HEIGHTS.start(), *HEIGHTS.end());
    Some((view.to_string(), height))
}

/// Draws an embedded document's content into the space the editor keeps for it, or `None` for
/// the editor's placeholder. Called each time the document is drawn.
pub(crate) type EmbedFn = dyn Fn(&Embed, &mut Window, &mut App) -> Option<AnyElement> + Send + Sync;

/// An embed being resized: its parse key, its Markdown as it was, its height then, and where the
/// press was.
#[derive(Clone, Debug)]
pub(crate) struct ResizeStart {
    pub(crate) key: u64,
    pub(crate) markdown: String,
    pub(crate) embed: Embed,
    pub(crate) title: SharedString,
    pub(crate) at: Point<Pixels>,
}

/// Embeds' heights while one is being resized, by parse key.
pub(crate) type Heights = Arc<std::sync::Mutex<std::collections::HashMap<u64, u32>>>;

/// Called when an embed's bottom edge is pressed, to follow the drag.
pub(crate) type OnResize = Arc<dyn Fn(ResizeStart, &mut Window, &mut App) + Send + Sync>;

/// What embeds draw and do.
#[derive(Clone, Default)]
pub(crate) struct EmbedHooks {
    pub(crate) docs: Option<Arc<dyn Docs>>,
    pub(crate) render: Option<Arc<EmbedFn>>,
    pub(crate) on_click: Option<OnDocClick>,
    /// Heights while a resize is under way, by parse key.
    pub(crate) heights: Heights,
    pub(crate) on_resize: Option<OnResize>,
    /// A block's lasting id, as [`Editor::document`](crate::Editor::document) gives it.
    pub(crate) lasting_id: Option<LastingId>,
}

/// Gives a block its lasting id.
pub(crate) type LastingId = Arc<dyn Fn(markdown_editor::BlockId) -> String + Send + Sync>;

/// An embed as the document keeps it.
#[derive(Clone, Debug, PartialEq)]
struct EmbedRef {
    /// New each time the block is read, so a resize finds its block's drawing.
    key: u64,
    embed: Embed,
    title: SharedString,
    markdown: String,
}

static EMBED_KEYS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The custom node's name for an embed.
pub(crate) const EMBED_NODE: &str = "doc-embed";

/// Embed blocks: a line holding only `[title](doc:id "embed view height")`.
pub(crate) struct EmbedPlugin(pub(crate) EmbedHooks);

impl Extension for EmbedPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        EMBED_NODE
    }

    fn parse(&self, node: &Node) -> Option<CustomNode> {
        let Node::Paragraph(paragraph) = node else {
            return None;
        };
        let [Node::Link(link)] = paragraph.children.as_slice() else {
            return None;
        };
        let (view, height) = embed_title(link.title.as_deref()?)?;
        let doc = DocRef::from_link(link)?;
        let markdown = embed(&doc.id, &doc.title, &view, height);
        let data = EmbedRef {
            key: EMBED_KEYS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            embed: Embed {
                doc: doc.id.clone(),
                view: view.into(),
                height,
                block: SharedString::default(),
            },
            title: doc.title.clone(),
            markdown: markdown.clone(),
        };
        let label = format!("{} embedded", doc.title);
        Some(
            CustomNode::new(EMBED_NODE, data)
                .text(doc.title.clone())
                .accessibility_label(label)
                .markdown(markdown),
        )
    }

    fn render(&self, node: &CustomNode, window: &mut Window, cx: &mut App) -> AnyElement {
        self.draw(node, None, window, cx)
    }

    fn render_block(
        &self,
        node: &CustomNode,
        block: markdown_editor::BlockId,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.draw(node, Some(block), window, cx)
    }
}

impl EmbedPlugin {
    /// The embed, with its block's lasting id when the editor knows its block.
    fn draw(
        &self,
        node: &CustomNode,
        block: Option<markdown_editor::BlockId>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let Some(data) = node.data::<EmbedRef>() else {
            return div().into_any_element();
        };
        let hooks = &self.0;
        let height = hooks
            .heights
            .lock()
            .ok()
            .and_then(|h| h.get(&data.key).copied())
            .unwrap_or(data.embed.height);
        let shown = Embed {
            height,
            block: block
                .zip(hooks.lasting_id.as_ref())
                .map(|(block, id)| id(block).into())
                .unwrap_or_default(),
            ..data.embed.clone()
        };
        let doc_ref = DocRef {
            id: data.embed.doc.clone(),
            title: data.title.clone(),
        };
        let (title, icon, found) = DocHooks {
            docs: hooks.docs.clone(),
            on_click: None,
        }
        .info(&doc_ref, cx);
        let view = view_label(hooks.docs.as_deref(), &data.embed.doc, &data.embed.view, cx);
        let content = hooks.render.as_ref().and_then(|r| r(&shown, window, cx));
        let theme = cx.theme();
        let (muted, fg, border, accent) = (
            theme.muted_foreground,
            theme.foreground,
            theme.border,
            theme.accent,
        );
        let on_click = hooks.on_click.clone();
        let id = data.embed.doc.clone();
        let header = div()
            .id(SharedString::from(format!(
                "blocks-embed-title-{}",
                data.key
            )))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(10.))
            .h(px(32.))
            .border_b_1()
            .border_color(border)
            .text_size(px(13.))
            .cursor_pointer()
            .hover(move |s| s.bg(accent))
            .child(
                div()
                    .flex_none()
                    .child(icon.unwrap_or_else(|| PAGE_GLYPH.into())),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if found { fg } else { muted })
                    .child(title.clone()),
            )
            .children(
                view.clone()
                    .map(|view| div().flex_none().text_color(muted).child(view)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                if let Some(on_click) = &on_click {
                    on_click(&id, window, cx);
                }
            });
        let body = match content {
            Some(content) => div().flex_1().min_h_0().overflow_hidden().child(content),
            None => div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .text_color(muted)
                .child(match &view {
                    Some(view) => format!("{title} ({view}) isn't shown here"),
                    None => format!("{title} isn't shown here"),
                }),
        };
        let on_resize = hooks.on_resize.clone();
        let start = ResizeStart {
            key: data.key,
            markdown: data.markdown.clone(),
            embed: data.embed.clone(),
            title: data.title.clone(),
            at: Point::default(),
        };
        let grip = div()
            .id(SharedString::from(format!(
                "blocks-embed-grip-{}",
                data.key
            )))
            .debug_selector(|| "blocks-embed-grip".into())
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .h(px(8.))
            .cursor(CursorStyle::ResizeUpDown)
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                cx.stop_propagation();
                if let Some(on_resize) = &on_resize {
                    on_resize(
                        ResizeStart {
                            at: event.position,
                            ..start.clone()
                        },
                        window,
                        cx,
                    );
                }
            });
        let selector = format!("blocks-embed-{}", data.embed.doc);
        div()
            .debug_selector(move || selector)
            .relative()
            .w_full()
            .my(px(4.))
            .h(px(height as f32))
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(border)
            .overflow_hidden()
            .child(header)
            .child(body)
            .child(grip)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::rust_2024::test;

    #[test]
    fn ids_are_short_and_url_safe() {
        assert_eq!(doc_id("doc:4f2a-x_y.z~"), Some("4f2a-x_y.z~"));
        assert_eq!(doc_id("doc:"), None);
        assert_eq!(doc_id("doc:a b"), None);
        assert_eq!(doc_id("doc:a/b"), None);
        assert_eq!(doc_id("doc:é"), None);
        assert_eq!(doc_id("https://example.com"), None);
        assert_eq!(doc_id(&format!("doc:{}", "a".repeat(129))), None);
        assert!(doc_id(&format!("doc:{}", "a".repeat(128))).is_some());
    }

    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;

    use markdown_editor::{BlockEditorModel, BlockKind};

    use crate::{Editor, EditorEvent, Options};

    fn model(source: &str) -> BlockEditorModel {
        BlockEditorModel::from_markdown(source, crate::extensions(&[])).unwrap()
    }

    fn kinds(m: &BlockEditorModel) -> Vec<BlockKind> {
        m.document().root().children().map(|b| b.kind()).collect()
    }

    #[test]
    fn links_round_trip() {
        for source in [
            "See [Roadmap](doc:4f2a) for the dates.",
            "[Meeting notes](doc:9c1e \"page\")",
            "**[Roadmap](doc:4f2a)** and [Q3 \\[draft\\]](doc:b)",
            "- [Notes](doc:9c1e \"page\")\n- [Roadmap](doc:4f2a)",
            "> [Notes](doc:9c1e \"page\")",
            "[elsewhere](doc:a/b) and [web](https://example.com)",
        ] {
            let once = model(source).source();
            assert_eq!(once, source);
            assert_eq!(model(&once).source(), source, "stable after a second save");
        }
    }

    #[test]
    fn a_page_is_a_block_and_a_mention_is_text() {
        assert_eq!(
            kinds(&model("[Notes](doc:9c1e \"page\")")),
            [BlockKind::Custom]
        );
        let m = model("See [Roadmap](doc:4f2a).");
        assert_eq!(kinds(&m), [BlockKind::Paragraph]);
        let text = m.document().root().children().next().unwrap().text();
        assert_eq!(text, Some("See \u{FFFC}."), "the mention is one unit");
        // A link titled page among other text is a mention.
        let m = model("[Notes](doc:9c1e \"page\") and more");
        assert_eq!(kinds(&m), [BlockKind::Paragraph]);
        assert_eq!(m.source(), "[Notes](doc:9c1e) and more");
    }

    struct Titles(Mutex<Vec<(&'static str, &'static str)>>);

    impl Docs for Titles {
        fn info(&self, id: &str, _: &App) -> Option<DocInfo> {
            let titles = self.0.lock().unwrap();
            titles
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, title)| DocInfo::new(*title).icon("🗺"))
        }
    }

    struct Shared(std::sync::Arc<Titles>);

    impl Docs for Shared {
        fn info(&self, id: &str, cx: &App) -> Option<DocInfo> {
            self.0.info(id, cx)
        }
    }

    fn open<'a>(
        cx: &'a mut TestAppContext,
        source: &'static str,
        titles: std::sync::Arc<Titles>,
    ) -> (
        Entity<Editor>,
        Rc<RefCell<Vec<EditorEvent>>>,
        &'a mut VisualTestContext,
    ) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(move |window, cx| {
            Editor::with_options(source, Options::default().docs(Shared(titles)), window, cx)
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &EditorEvent, _| {
                seen.borrow_mut().push(event.clone())
            })
            .detach();
        });
        draw(cx);
        (editor, events, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    fn clicked(events: &RefCell<Vec<EditorEvent>>) -> Vec<String> {
        events
            .borrow()
            .iter()
            .filter_map(|e| match e {
                EditorEvent::DocClicked { id } => Some(id.to_string()),
                _ => None,
            })
            .collect()
    }

    #[gpui::test]
    fn clicking_a_link_tells_the_app(cx: &mut TestAppContext) {
        let titles = std::sync::Arc::new(Titles(Mutex::new(vec![
            ("4f2a", "Roadmap"),
            ("9c1e", "Notes"),
        ])));
        let (editor, events, cx) = open(
            cx,
            "See [Old name](doc:4f2a) here.\n\n[Notes](doc:9c1e \"page\")",
            titles,
        );
        let before = editor.read_with(cx, |e, cx| e.markdown(cx));
        let mention = cx
            .debug_bounds("blocks-mention-4f2a")
            .expect("the mention is drawn");
        cx.simulate_click(mention.center(), Modifiers::none());
        draw(cx);
        let page = cx
            .debug_bounds("blocks-page-9c1e")
            .expect("the page is drawn");
        cx.simulate_click(page.center(), Modifiers::none());
        draw(cx);
        assert_eq!(clicked(&events), ["4f2a", "9c1e"]);
        assert_eq!(editor.read_with(cx, |e, cx| e.markdown(cx)), before);
    }

    #[gpui::test]
    fn a_renamed_doc_is_drawn_with_its_new_title(cx: &mut TestAppContext) {
        let titles = std::sync::Arc::new(Titles(Mutex::new(vec![("4f2a", "Q3")])));
        let (editor, _, cx) = open(cx, "See [Q3](doc:4f2a) here.", titles.clone());
        let narrow = cx.debug_bounds("blocks-mention-4f2a").unwrap().size.width;
        titles.0.lock().unwrap()[0].1 = "The third quarter's roadmap";
        editor.update(cx, |e, cx| e.refresh_docs(cx));
        draw(cx);
        let wide = cx.debug_bounds("blocks-mention-4f2a").unwrap().size.width;
        assert!(wide > narrow * 2., "{narrow:?} became {wide:?}");
        // The file keeps the title it was written with.
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "See [Q3](doc:4f2a) here."
        );
    }

    #[gpui::test]
    fn the_slash_menu_asks_the_app_and_the_app_answers(cx: &mut TestAppContext) {
        let titles = std::sync::Arc::new(Titles(Mutex::new(vec![])));
        let (editor, events, cx) = open(cx, "Intro", titles);
        editor.update_in(cx, |e, window, cx| {
            e.focus(window, cx);
            e.state().update(cx, |s, cx| {
                let _ = s.select_end(cx);
            });
        });
        draw(cx);
        cx.simulate_keystrokes("enter");
        cx.simulate_input("/page");
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            events
                .borrow()
                .iter()
                .any(|e| matches!(e, EditorEvent::NewPageRequested)),
            "{:?}",
            events.borrow()
        );
        // The empty line stays, for the page to go in.
        assert_eq!(editor.read_with(cx, |e, cx| e.markdown(cx)), "Intro\n\n");
        editor.update(cx, |e, cx| e.insert_page("n1", "New page", cx));
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[New page](doc:n1 \"page\")\n\n"
        );
        // A mention goes in at the caret, which is on the new line after the page.
        editor.update(cx, |e, cx| e.insert_mention("4f2a", "Roadmap", cx));
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[New page](doc:n1 \"page\")\n\n[Roadmap](doc:4f2a)"
        );
        // One undo takes the mention back out whole.
        editor.update(cx, |e, cx| e.state().update(cx, |s, cx| s.undo(cx)));
        assert!(
            !editor
                .read_with(cx, |e, cx| e.markdown(cx))
                .contains("Roadmap")
        );
        // Bad ids insert nothing.
        let before = editor.read_with(cx, |e, cx| e.markdown(cx));
        editor.update(cx, |e, cx| e.insert_mention("../etc", "x", cx));
        assert_eq!(editor.read_with(cx, |e, cx| e.markdown(cx)), before);
    }

    fn open_with_embeds<'a>(
        cx: &'a mut TestAppContext,
        source: &'static str,
        drawn: std::sync::Arc<Mutex<Vec<Embed>>>,
    ) -> (
        Entity<Editor>,
        Rc<RefCell<Vec<EditorEvent>>>,
        &'a mut VisualTestContext,
    ) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(move |window, cx| {
            let options = Options::default()
                .docs(Shared(std::sync::Arc::new(Titles(Mutex::new(vec![(
                    "b1", "Q4 board",
                )])))))
                .embeds(move |embed, _, _| {
                    drawn.lock().unwrap().push(embed.clone());
                    (embed.view == "board").then(|| {
                        div()
                            .debug_selector(|| "host-board".into())
                            .size_full()
                            .into_any_element()
                    })
                });
            Editor::with_options(source, options, window, cx)
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &EditorEvent, _| {
                seen.borrow_mut().push(event.clone())
            })
            .detach();
        });
        draw(cx);
        (editor, events, cx)
    }

    /// Each embed tells the app its block's lasting id, the one the document gives it, so two
    /// embeds of one database showing the same view are told apart; the id lasts while the
    /// document is edited.
    #[gpui::test]
    fn each_embed_tells_its_lasting_block_id(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (editor, _, cx) = open_with_embeds(
            cx,
            "[Board](doc:b1 \"embed board 300\")\n\nBetween\n\n[Board](doc:b1 \"embed board 300\")",
            drawn.clone(),
        );
        let blocks = |drawn: &Mutex<Vec<Embed>>| -> Vec<String> {
            let mut ids: Vec<String> = drawn
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.block.to_string())
                .collect();
            ids.sort();
            ids.dedup();
            ids
        };
        let first = blocks(&drawn);
        assert_eq!(first.len(), 2, "two embeds, two ids: {first:?}");
        assert!(first.iter().all(|id| !id.is_empty()));
        // The same ids the document gives those blocks.
        let doc = editor.update(cx, |e, cx| e.document(cx));
        let mut doc_ids = vec![doc.blocks[0].id.clone(), doc.blocks[2].id.clone()];
        doc_ids.sort();
        assert_eq!(first, doc_ids);
        // Typing elsewhere keeps them.
        drawn.lock().unwrap().clear();
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let leaf = s
                    .model()
                    .document()
                    .leaves()
                    .find(|l| l.text() == Some("Between"))
                    .unwrap()
                    .id();
                let _ = s.set_selection(
                    markdown_editor::BlockSelection::caret(markdown_editor::BlockPosition::new(
                        leaf, 7,
                    )),
                    cx,
                );
            })
        });
        cx.simulate_input("!");
        draw(cx);
        assert!(
            editor
                .update(cx, |e, cx| e.markdown(cx))
                .contains("Between!")
        );
        assert_eq!(blocks(&drawn), first);
    }

    /// An embed's header names its view as the app does, else a built-in kind by its name, and
    /// never shows a view id the app couldn't name.
    #[gpui::test]
    fn an_embeds_header_names_its_view(cx: &mut TestAppContext) {
        struct Views;
        impl Docs for Views {
            fn info(&self, _: &str, _: &App) -> Option<DocInfo> {
                None
            }
            fn view_name(&self, doc: &str, view: &str, _: &App) -> Option<SharedString> {
                (doc == "db" && view == "v-board").then(|| "Launch tasks".into())
            }
        }
        cx.update(|cx| {
            let views: &dyn Docs = &Views;
            let label = |docs, doc, view| view_label(docs, doc, view, cx).map(|v| v.to_string());
            assert_eq!(
                label(Some(views), "db", "v-board").as_deref(),
                Some("Launch tasks")
            );
            assert_eq!(
                label(Some(views), "db", "8b1f3c2e-0d4a-4e8f-9f7a-1c2d3e4f5a6b"),
                None
            );
            assert_eq!(label(Some(views), "db", "board").as_deref(), Some("Board"));
            assert_eq!(label(None, "db", "mindmap").as_deref(), Some("Mind map"));
            assert_eq!(label(None, "db", "v-board"), None);
        });
    }

    #[gpui::test]
    fn an_embed_is_drawn_by_the_app_in_the_space_it_keeps(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (_, events, cx) = open_with_embeds(
            cx,
            "Intro\n\n[Board](doc:b1 \"embed board 300\")\n\n[Map](doc:m1 \"embed mindmap 200\")",
            drawn.clone(),
        );
        let asked: Vec<(String, String, u32)> = drawn
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.doc.to_string(), e.view.to_string(), e.height))
            .collect();
        assert!(asked.contains(&("b1".into(), "board".into(), 300)));
        assert!(asked.contains(&("m1".into(), "mindmap".into(), 200)));
        // The board is the app's; the mind map, which the app doesn't draw, is a placeholder.
        assert!(cx.debug_bounds("host-board").is_some());
        let embed = cx.debug_bounds("blocks-embed-b1").unwrap();
        assert_eq!(embed.size.height, px(300.));
        // A click on the title opens the document.
        cx.simulate_click(
            point(embed.left() + px(40.), embed.top() + px(16.)),
            Modifiers::none(),
        );
        draw(cx);
        assert_eq!(clicked(&events), ["b1"]);
    }

    #[gpui::test]
    fn dragging_an_embeds_edge_sets_its_height(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (editor, _, cx) = open_with_embeds(
            cx,
            "Intro\n\n[Board](doc:b1 \"embed board 300\")\n\nAfter",
            drawn,
        );
        let grip = cx.debug_bounds("blocks-embed-grip").unwrap().center();
        cx.simulate_mouse_down(grip, MouseButton::Left, Modifiers::none());
        draw(cx);
        cx.simulate_mouse_move(
            grip + point(px(0.), px(90.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        draw(cx);
        // Taller while dragging, before anything is written.
        assert_eq!(
            cx.debug_bounds("blocks-embed-b1").unwrap().size.height,
            px(390.)
        );
        cx.simulate_mouse_move(
            grip + point(px(0.), px(180.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            grip + point(px(0.), px(180.)),
            MouseButton::Left,
            Modifiers::none(),
        );
        draw(cx);
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Board](doc:b1 \"embed board 480\")\n\nAfter"
        );
        assert_eq!(
            cx.debug_bounds("blocks-embed-b1").unwrap().size.height,
            px(480.)
        );
        // One undo puts it back.
        editor.update(cx, |e, cx| e.state().update(cx, |s, cx| s.undo(cx)));
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Board](doc:b1 \"embed board 300\")\n\nAfter"
        );
    }

    #[gpui::test]
    fn the_slash_menu_asks_for_an_embed(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (editor, events, cx) = open_with_embeds(cx, "Intro", drawn);
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let _ = s.select_end(cx);
            })
        });
        cx.simulate_keystrokes("enter");
        cx.simulate_input("/embed");
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert!(
            events
                .borrow()
                .iter()
                .any(|e| matches!(e, EditorEvent::EmbedRequested))
        );
        editor.update(cx, |e, cx| e.insert_embed("b1", "Q4 board", "board", cx));
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Q4 board](doc:b1 \"embed board 320\")\n\n"
        );
    }

    /// Pastes `text` as the editor's own copy of a line's content would be.
    fn paste(cx: &mut VisualTestContext, text: &str) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string_with_metadata(
            text.to_string(),
            "gpui-kit/block-editor/inline-markdown/v1".to_string(),
        ));
        keys_paste(cx);
    }

    /// Pastes `text` as another app's would be.
    fn paste_external(cx: &mut VisualTestContext, text: &str) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_string()));
        keys_paste(cx);
    }

    fn keys_paste(cx: &mut VisualTestContext) {
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-v"
        } else {
            "ctrl-v"
        });
        draw(cx);
    }

    fn new_line(editor: &Entity<Editor>, cx: &mut VisualTestContext) {
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let _ = s.select_end(cx);
            })
        });
        cx.simulate_keystrokes("enter");
        draw(cx);
    }

    #[gpui::test]
    fn a_pasted_doc_link_can_become_an_embed(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (editor, _, cx) = open_with_embeds(cx, "Intro", drawn);
        new_line(&editor, cx);
        paste(cx, "[Board](doc:b1)");
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Board](doc:b1)"
        );
        // Already a mention: the offer is a page or an embed.
        assert!(cx.debug_bounds("blocks-paste-Mention").is_none());
        let embed = cx
            .debug_bounds("blocks-paste-Embed")
            .expect("offered")
            .center();
        cx.simulate_click(embed, Modifiers::none());
        draw(cx);
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Board](doc:b1 \"embed page 320\")\n\n"
        );
        assert!(
            cx.debug_bounds("blocks-paste-Embed").is_none(),
            "the offer closes"
        );
        // One undo takes it back to the pasted link.
        editor.update(cx, |e, cx| e.state().update(cx, |s, cx| s.undo(cx)));
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Board](doc:b1)"
        );
    }

    #[gpui::test]
    fn a_pasted_doc_address_can_become_a_page_titled_by_the_app(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (editor, _, cx) = open_with_embeds(cx, "Intro", drawn);
        new_line(&editor, cx);
        paste_external(cx, "doc:b1");
        assert!(cx.debug_bounds("blocks-paste-Mention").is_some());
        let page = cx.debug_bounds("blocks-paste-Page").unwrap().center();
        cx.simulate_click(page, Modifiers::none());
        draw(cx);
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Q4 board](doc:b1 \"page\")\n\n"
        );
    }

    #[gpui::test]
    fn a_link_pasted_from_another_app_can_become_a_mention(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (editor, _, cx) = open_with_embeds(cx, "Intro", drawn);
        new_line(&editor, cx);
        paste_external(cx, "[Board](doc:b1)");
        let mention = cx
            .debug_bounds("blocks-paste-Mention")
            .expect("offered")
            .center();
        cx.simulate_click(mention, Modifiers::none());
        draw(cx);
        assert_eq!(
            editor.read_with(cx, |e, cx| e.markdown(cx)),
            "Intro\n\n[Board](doc:b1)"
        );
    }

    #[gpui::test]
    fn no_offer_for_a_link_among_words_or_after_typing(cx: &mut TestAppContext) {
        let drawn = std::sync::Arc::new(Mutex::new(Vec::new()));
        let (editor, _, cx) = open_with_embeds(cx, "Intro", drawn);
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let _ = s.select_end(cx);
            })
        });
        paste(cx, " [Board](doc:b1)");
        assert!(cx.debug_bounds("blocks-paste-Page").is_none());
        new_line(&editor, cx);
        paste(cx, "[Board](doc:b1)");
        assert!(cx.debug_bounds("blocks-paste-Page").is_some());
        cx.simulate_input("x");
        draw(cx);
        assert!(cx.debug_bounds("blocks-paste-Page").is_none());
    }

    #[test]
    fn a_database_views_id_is_a_view() {
        let uuid = "3f2b8c1e-9a4d-4e6f-b1c2-0d9e8f7a6b5C";
        assert_eq!(
            embed("a", "T", uuid, 300),
            format!("[T](doc:a \"embed {uuid} 300\")")
        );
        let too_long = "v".repeat(65);
        assert_eq!(
            embed("a", "T", &too_long, 300),
            "[T](doc:a \"embed page 300\")"
        );
    }

    #[test]
    fn embed_titles_are_read_and_kept_in_bounds() {
        assert_eq!(embed("a", "T", "Board!", 5), "[T](doc:a \"embed page 80\")");
        assert_eq!(
            embed("a", "T", "table", 99999),
            "[T](doc:a \"embed table 1600\")"
        );
        assert_eq!(embed_title("embed"), Some(("page".into(), 320)));
        assert_eq!(
            embed_title("embed calendar 90"),
            Some(("calendar".into(), 90))
        );
        assert_eq!(embed_title("embedded"), None);
        assert_eq!(embed_title("page"), None);
    }

    #[test]
    fn titles_are_escaped_and_kept_on_one_line() {
        assert_eq!(mention("a", "Q3 [draft]"), r"[Q3 \[draft\]](doc:a)");
        assert_eq!(mention("a", "  "), "[Untitled](doc:a)");
        assert_eq!(mention("a", "Done!"), "[Done!](doc:a)");
        assert_eq!(page("a", "one\ntwo"), "[one two](doc:a \"page\")");
    }
}
