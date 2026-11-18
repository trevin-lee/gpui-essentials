//! Bookmarks: a web page as a card, Notion's web bookmark, with its title, description and
//! picture.
//!
//! In Markdown a bookmark is a line holding only a link titled `bookmark`, then, when it has a
//! description or picture, a comment holding them as JSON, which GitHub doesn't show:
//!
//! ```markdown
//! [The page's title](https://example.com/a "bookmark") <!-- {"description":"What it says.","image":"asset:…"} -->
//! ```
//!
//! The editor never fetches a page. The app's unfurl ([`crate::Options::unfurl`]) asks its
//! server for the title, description and picture when a bookmark is made, and the card shows
//! the address until the answer comes. The picture is the app's own stored copy, an
//! `asset:<sha256>`, which loads through [`crate::Options::images`]; a card never draws a
//! picture from anywhere else.

use std::sync::Arc;

use gpui::*;
use gpui_core::ActiveTheme as _;
use markdown::mdast::Node;
use markdown_editor::{CustomNode, Extension};

/// The link title that makes a link on a line of its own a bookmark.
pub const BOOKMARK_TITLE: &str = "bookmark";

/// The custom node's name for a bookmark.
pub(crate) const BOOKMARK_NODE: &str = "web-bookmark";

/// The most characters of a description written.
pub const MAX_DESCRIPTION: usize = 300;

/// What the app's server says about a page, for its bookmark: each part only when the page
/// has one. `image` is an `asset:<sha256>` the app stored; anything else isn't drawn or
/// written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Unfurl {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

/// Asks the app about a page, for a new bookmark (see [`crate::Options::unfurl`]).
pub(crate) type UnfurlFn = Arc<dyn Fn(&str, &mut App) -> Task<Option<Unfurl>>>;

/// A bookmark as its Markdown holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bookmark {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

/// Whether `url` can be a bookmark's: an `http` or `https` address with no spaces or control
/// characters.
pub fn web_url(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && url.len() > "http://".len()
        && !url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>'))
}

/// Whether `image` is a picture a bookmark may have: `asset:` and a SHA-256 in lowercase hex.
pub fn asset_image(image: &str) -> bool {
    image.strip_prefix("asset:").is_some_and(|hash| {
        hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

impl Bookmark {
    /// A bookmark of `url` with nothing known about it yet.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Self::default()
        }
    }

    /// This bookmark with what the app's server said about its page.
    pub fn unfurled(mut self, unfurl: Unfurl) -> Self {
        let some = |s: Option<String>| s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        self.title = some(unfurl.title).or(self.title);
        self.description = some(unfurl.description).or(self.description);
        self.image = unfurl.image.filter(|i| asset_image(i)).or(self.image);
        self
    }

    /// Its Markdown: the link, then the comment when it has a description or picture. Only
    /// a picture that's an asset is written, and a description of at most
    /// [`MAX_DESCRIPTION`] characters on one line.
    pub fn markdown(&self) -> String {
        let title = self
            .title
            .as_deref()
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|t| !t.is_empty());
        let text = crate::docs::link_text(title.as_deref().unwrap_or(&self.url));
        // Parentheses would end a bare destination early.
        let url = if self.url.contains(['(', ')']) {
            format!("<{}>", self.url)
        } else {
            self.url.clone()
        };
        let mut out = format!("[{text}]({url} \"{BOOKMARK_TITLE}\")");
        let description = self
            .description
            .as_deref()
            .map(|d| {
                d.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(MAX_DESCRIPTION)
                    .collect::<String>()
            })
            .filter(|d| !d.is_empty());
        let image = self.image.as_deref().filter(|i| asset_image(i));
        if description.is_some() || image.is_some() {
            let mut fields = serde_json::Map::new();
            if let Some(description) = description {
                fields.insert("description".into(), description.into());
            }
            if let Some(image) = image {
                fields.insert("image".into(), image.into());
            }
            let json = serde_json::Value::Object(fields).to_string();
            // Escaped so the JSON can't end the comment.
            let json = json
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
                .replace('&', "\\u0026");
            out.push_str(&format!(" <!-- {json} -->"));
        }
        out
    }

    /// A bookmark read back from a paragraph's nodes: one link titled `bookmark` to a web
    /// address, alone or with the comment after it. A comment that isn't the bookmark's (not
    /// a JSON object of `description` and `image` strings) makes it an ordinary paragraph.
    fn of_paragraph(children: &[Node]) -> Option<Self> {
        let (link, comment) = match children {
            [Node::Link(link)] => (link, None),
            [Node::Link(link), Node::Text(space), Node::Html(html)]
                if space.value.trim().is_empty() =>
            {
                (link, Some(html.value.as_str()))
            }
            _ => return None,
        };
        if link.title.as_deref() != Some(BOOKMARK_TITLE) || !web_url(&link.url) {
            return None;
        }
        let text = plain_text(&link.children);
        // The address as its text means no title. An address read as a link inside the link
        // keeps the backslashes its text was written with.
        let title = (!text.is_empty() && text.replace('\\', "") != link.url.replace('\\', ""))
            .then_some(text);
        let mut bookmark = Self {
            url: link.url.clone(),
            title,
            ..Self::default()
        };
        if let Some(comment) = comment {
            let json = comment
                .trim()
                .strip_prefix("<!--")?
                .strip_suffix("-->")?
                .trim();
            let serde_json::Value::Object(fields) = serde_json::from_str(json).ok()? else {
                return None;
            };
            for (key, value) in fields {
                let serde_json::Value::String(value) = value else {
                    return None;
                };
                match key.as_str() {
                    // Read whole, however long: only writing caps it.
                    "description" => bookmark.description = Some(value),
                    // A picture from anywhere else is never drawn.
                    "image" => bookmark.image = Some(value).filter(|i| asset_image(i)),
                    _ => return None,
                }
            }
        }
        Some(bookmark)
    }

    /// A bookmark read back from a block's Markdown, as [`Bookmark::markdown`] writes it.
    pub fn of_markdown(markdown: &str) -> Option<Self> {
        let tree =
            markdown_editor::parse_markdown(markdown, &markdown::ParseOptions::gfm()).ok()?;
        let [Node::Paragraph(paragraph)] = tree.children()?.as_slice() else {
            return None;
        };
        Self::of_paragraph(&paragraph.children)
    }

    /// The page's site, as the card shows it: the host without `www.`.
    pub fn site(&self) -> &str {
        let rest = self.url.split_once("://").map_or(&*self.url, |(_, r)| r);
        let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
        let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
        host.strip_prefix("www.").unwrap_or(host)
    }
}

fn plain_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.value),
            Node::InlineCode(c) => out.push_str(&c.value),
            other => {
                if let Some(children) = other.children() {
                    out.push_str(&plain_text(children));
                }
            }
        }
    }
    out
}

/// A click on a card, with its page's address.
pub(crate) type OnClick = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;

/// What a bookmark's card needs from the editor.
#[derive(Clone, Default)]
pub(crate) struct BookmarkHooks {
    /// The app's picture loader, for the card's picture.
    pub(crate) images: Option<markdown_editor::ImageSourceFn>,
    /// A click on the card: the app opens the page.
    pub(crate) on_click: Option<OnClick>,
}

/// Bookmark blocks: a line holding only `[title](url "bookmark")`, maybe with its comment.
pub(crate) struct BookmarkPlugin(pub(crate) BookmarkHooks);

impl Extension for BookmarkPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        BOOKMARK_NODE
    }

    fn parse(&self, node: &Node) -> Option<CustomNode> {
        let Node::Paragraph(paragraph) = node else {
            return None;
        };
        let bookmark = Bookmark::of_paragraph(&paragraph.children)?;
        let markdown = bookmark.markdown();
        let shown = bookmark
            .title
            .clone()
            .unwrap_or_else(|| bookmark.url.clone());
        let label = format!("Bookmark {shown}");
        Some(
            CustomNode::new(BOOKMARK_NODE, bookmark)
                .text(shown)
                .accessibility_label(label)
                .markdown(markdown),
        )
    }

    fn render(&self, node: &CustomNode, _: &mut Window, cx: &mut App) -> AnyElement {
        let Some(bookmark) = node.data::<Bookmark>() else {
            return div().into_any_element();
        };
        card(bookmark, &self.0, cx)
    }
}

/// A bookmark's card, as Notion's: the title, two lines of the description and the site on the
/// left, the picture on the right.
fn card(bookmark: &Bookmark, hooks: &BookmarkHooks, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let (fg, muted, border, accent) = (
        theme.foreground,
        theme.muted_foreground,
        theme.border,
        theme.accent,
    );
    let title = bookmark
        .title
        .clone()
        .unwrap_or_else(|| bookmark.url.clone());
    let picture = bookmark
        .image
        .as_deref()
        .filter(|i| asset_image(i))
        .zip(hooks.images.as_ref())
        .map(|(image, load)| load(&SharedUri::from(image.to_string())));
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(14.))
        .py(px(12.))
        .child(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(fg)
                .text_size(px(14.))
                .child(title.clone()),
        )
        .children(bookmark.description.clone().map(|description| {
            div()
                .text_color(muted)
                .text_size(px(12.))
                .line_clamp(2)
                .child(description)
        }))
        .child(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(fg)
                .text_size(px(12.))
                .child(bookmark.url.clone()),
        );
    let on_click = hooks.on_click.clone();
    let url = bookmark.url.clone();
    div()
        .id(SharedString::from(format!(
            "blocks-bookmark-{}",
            bookmark.url
        )))
        .debug_selector(|| "blocks-bookmark".into())
        .role(Role::Link)
        .aria_label(format!("Bookmark {title}, {}", bookmark.site()))
        .w_full()
        .my(px(4.))
        .flex()
        .overflow_hidden()
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .cursor_pointer()
        .hover(move |s| s.bg(accent))
        .child(text)
        .children(picture.map(|source| {
            div()
                .flex_none()
                .w(relative(0.33))
                .max_w(px(220.))
                .min_h(px(96.))
                .border_l_1()
                .border_color(border)
                .child(img(source).size_full().object_fit(ObjectFit::Cover))
        }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            if let Some(on_click) = &on_click {
                on_click(&url, window, cx);
            }
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::rust_2024::test;

    const IMAGE: &str = "asset:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn a_bookmark_writes_and_reads_back() {
        let bookmark = Bookmark {
            url: "https://example.com/a".into(),
            title: Some("A [page]".into()),
            description: Some("It says <b>this</b> & that.".into()),
            image: Some(IMAGE.into()),
        };
        let markdown = bookmark.markdown();
        assert_eq!(
            markdown,
            format!(
                "[A \\[page\\]](https://example.com/a \"bookmark\") <!-- {{\"description\":\"It says \\u003cb\\u003ethis\\u003c/b\\u003e \\u0026 that.\",\"image\":\"{IMAGE}\"}} -->"
            )
        );
        assert_eq!(Bookmark::of_markdown(&markdown), Some(bookmark));
        // Nothing known: the bare link, with the address as its text.
        let bare = Bookmark::new("https://example.com/b");
        assert_eq!(
            bare.markdown(),
            "[https://example.com/b](https://example.com/b \"bookmark\")"
        );
        assert_eq!(Bookmark::of_markdown(&bare.markdown()), Some(bare));
        let parens = Bookmark::new("https://en.wikipedia.org/wiki/Rust_(language)");
        assert_eq!(Bookmark::of_markdown(&parens.markdown()), Some(parens));
    }

    #[test]
    fn only_assets_and_short_descriptions_are_written() {
        let bookmark = Bookmark {
            url: "https://example.com".into(),
            title: None,
            description: Some("x".repeat(400)),
            image: Some("https://example.com/a.png".into()),
        };
        let back = Bookmark::of_markdown(&bookmark.markdown()).unwrap();
        assert_eq!(
            back.description.map(|d| d.chars().count()),
            Some(MAX_DESCRIPTION)
        );
        assert_eq!(back.image, None);
        // Read tolerantly: a longer description stays whole until written.
        let long = format!(
            "[a](https://example.com \"bookmark\") <!-- {{\"description\":\"{}\"}} -->",
            "y".repeat(500)
        );
        let read = Bookmark::of_markdown(&long).unwrap();
        assert_eq!(read.description.map(|d| d.len()), Some(500));
        // A picture that isn't an asset is never kept.
        let url_image =
            "[a](https://example.com \"bookmark\") <!-- {\"image\":\"https://x.test/i.png\"} -->";
        assert_eq!(Bookmark::of_markdown(url_image).unwrap().image, None);
    }

    #[test]
    fn anything_else_is_not_a_bookmark() {
        for markdown in [
            "[a](https://example.com)",
            "[a](https://example.com \"page\")",
            "[a](doc:abc \"bookmark\")",
            "[a](javascript:alert(1) \"bookmark\")",
            "[a](https://example.com \"bookmark\") and more",
            "[a](https://example.com \"bookmark\") <!-- not json -->",
            "[a](https://example.com \"bookmark\") <!-- {\"other\":\"x\"} -->",
            "[a](https://example.com \"bookmark\") <!-- {\"description\":3} -->",
            "[a](https://example.com \"bookmark\") <!-- [\"description\"] -->",
        ] {
            assert_eq!(Bookmark::of_markdown(markdown), None, "{markdown}");
        }
    }

    #[test]
    fn an_unfurl_fills_in_what_it_knows() {
        let bookmark = Bookmark::new("https://www.example.com/a?b#c").unfurled(Unfurl {
            title: Some("  Title ".into()),
            description: Some(String::new()),
            image: Some("https://example.com/i.png".into()),
        });
        assert_eq!(bookmark.title.as_deref(), Some("Title"));
        assert_eq!(bookmark.description, None);
        assert_eq!(bookmark.image, None);
        assert_eq!(bookmark.site(), "example.com");
    }

    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

    use crate::{Editor, EditorEvent, Options};

    /// The fake server's answers, sent when a test says so.
    type Answers = Rc<RefCell<Vec<(String, futures::channel::oneshot::Sender<Option<Unfurl>>)>>>;

    fn open<'a>(
        cx: &'a mut TestAppContext,
        source: &'static str,
    ) -> (
        Entity<Editor>,
        Answers,
        Rc<RefCell<Vec<EditorEvent>>>,
        &'a mut VisualTestContext,
    ) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let answers: Answers = Rc::default();
        let asked = answers.clone();
        let (editor, cx) = cx.add_window_view(move |window, cx| {
            let options = Options::default().unfurl(move |url, cx| {
                let (tx, rx) = futures::channel::oneshot::channel();
                asked.borrow_mut().push((url.to_string(), tx));
                cx.background_spawn(async move { rx.await.ok().flatten() })
            });
            Editor::with_options(source, options, window, cx)
        });
        let events: Rc<RefCell<Vec<EditorEvent>>> = Rc::default();
        let seen = events.clone();
        cx.update(|window, cx| {
            cx.subscribe(&editor, move |_, e: &EditorEvent, _| {
                seen.borrow_mut().push(e.clone())
            })
            .detach();
            editor.update(cx, |e, cx| e.focus(window, cx));
        });
        draw(cx);
        (editor, answers, events, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    fn markdown(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
        editor.read_with(cx, |e, cx| e.markdown(cx))
    }

    fn answer(answers: &Answers, unfurl: Option<Unfurl>) {
        let (_, tx) = answers.borrow_mut().remove(0);
        let _ = tx.send(unfurl);
    }

    #[gpui::test]
    fn a_bookmark_shows_the_address_until_the_server_answers(cx: &mut TestAppContext) {
        let (editor, answers, _, cx) = open(cx, "Intro\n\n");
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let _ = s.select_end(cx);
            });
            assert!(e.insert_bookmark("https://example.com/a", cx));
        });
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "Intro\n\n[https://example.com/a](https://example.com/a \"bookmark\")\n\n"
        );
        assert!(
            cx.debug_bounds("blocks-bookmark").is_some(),
            "drawn as a card"
        );
        assert_eq!(answers.borrow()[0].0, "https://example.com/a");
        // The answer fills it in, and a picture from the web is never written.
        answer(
            &answers,
            Some(Unfurl {
                title: Some("A page".into()),
                description: Some("What it says.".into()),
                image: Some("https://example.com/i.png".into()),
            }),
        );
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "Intro\n\n[A page](https://example.com/a \"bookmark\") <!-- {\"description\":\"What it says.\"} -->\n\n"
        );
        // No answer: it stays the address.
        editor.update(cx, |e, cx| {
            assert!(e.insert_bookmark("https://example.com/b", cx));
            assert!(!e.insert_bookmark("ftp://example.com/c", cx));
        });
        answer(&answers, None);
        draw(cx);
        assert!(
            markdown(&editor, cx)
                .contains("[https://example.com/b](https://example.com/b \"bookmark\")"),
            "{}",
            markdown(&editor, cx)
        );
    }

    #[gpui::test]
    fn a_pasted_web_address_can_become_a_bookmark(cx: &mut TestAppContext) {
        let (editor, answers, events, cx) = open(cx, "Intro");
        editor.update(cx, |e, cx| {
            e.state().update(cx, |s, cx| {
                let _ = s.select_end(cx);
            })
        });
        cx.simulate_keystrokes("enter");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            "https://example.com/a".to_string(),
        ));
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-v"
        } else {
            "ctrl-v"
        });
        draw(cx);
        assert!(cx.debug_bounds("blocks-paste-Page").is_none());
        let bookmark = cx
            .debug_bounds("blocks-paste-Bookmark")
            .expect("offered")
            .center();
        cx.simulate_click(bookmark, Modifiers::none());
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "Intro\n\n[https://example.com/a](https://example.com/a \"bookmark\")\n\n"
        );
        answer(
            &answers,
            Some(Unfurl {
                title: Some("A page".into()),
                ..Unfurl::default()
            }),
        );
        draw(cx);
        assert!(
            markdown(&editor, cx)
                .starts_with("Intro\n\n[A page](https://example.com/a \"bookmark\")")
        );
        // A click on the card asks the app to open the page.
        let card = cx.debug_bounds("blocks-bookmark").unwrap().center();
        cx.simulate_click(card, Modifiers::none());
        draw(cx);
        assert!(events.borrow().iter().any(|e| matches!(
            e,
            EditorEvent::LinkClicked { url } if url.as_ref() == "https://example.com/a"
        )));
    }
}
