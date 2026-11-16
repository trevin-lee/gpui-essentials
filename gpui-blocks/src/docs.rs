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

use gpui_kit::base::text::{
    InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use markdown::mdast::{Link, Node};

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
fn link_text(title: &str) -> String {
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
fn plain_text(nodes: &[Node]) -> String {
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

impl MarkdownPlugin for MentionPlugin {
    fn name(&self) -> &str {
        MENTION
    }

    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Link(link) = node else {
            return None;
        };
        let doc = DocRef::from_link(link)?;
        let label = format!("Link to {}", doc.title);
        // Copy as the title, written back as the link.
        let markdown = mention(&doc.id, &doc.title);
        Some(
            MarkdownNode::new(MENTION, doc.clone())
                .text(doc.title.clone())
                .accessibility_label(label)
                .markdown(markdown),
        )
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let doc = node.data::<DocRef>()?;
        let (title, icon, found) = self.0.info(doc, cx);
        let theme = cx.theme();
        let accent = theme.accent;
        let size = context.font_size();
        let line = context.line_height();
        let chip = div()
            .debug_selector(|| format!("blocks-mention-{}", doc.id))
            .id(SharedString::from(format!("blocks-mention-{}", doc.id)))
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
        Some(InlineElement::new(chip).with_baseline((line + size * 0.7) / 2.))
    }
}

/// Sub-page blocks: a line holding only `[title](doc:id "page")`.
pub(crate) struct PagePlugin(pub(crate) DocHooks);

impl MarkdownPlugin for PagePlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        PAGE
    }

    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
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
            MarkdownNode::new(PAGE, doc.clone())
                .text(doc.title.clone())
                .accessibility_label(label)
                .markdown(markdown),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(doc) = node.data::<DocRef>() else {
            return div().into_any_element();
        };
        let (title, icon, found) = self.0.info(doc, cx);
        let theme = cx.theme();
        let accent = theme.accent;
        div()
            .debug_selector(|| format!("blocks-page-{}", doc.id))
            .id(SharedString::from(format!("blocks-page-{}", doc.id)))
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
            gpui_kit::init(cx);
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

    #[gpui_kit::test]
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

    #[gpui_kit::test]
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

    #[gpui_kit::test]
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

    #[test]
    fn titles_are_escaped_and_kept_on_one_line() {
        assert_eq!(mention("a", "Q3 [draft]"), r"[Q3 \[draft\]](doc:a)");
        assert_eq!(mention("a", "  "), "[Untitled](doc:a)");
        assert_eq!(mention("a", "Done!"), "[Done!](doc:a)");
        assert_eq!(page("a", "one\ntwo"), "[one two](doc:a \"page\")");
    }
}
