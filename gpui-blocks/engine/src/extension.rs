// Added in Trevin Lee's fork (2026): the engine's own extension API.
//! Blocks and inline pieces a host defines: a widget, a link to another document, a table of
//! contents. An [`Extension`] reads them from the Markdown parser's syntax tree and draws them;
//! the editor keeps them in the document as [`CustomNode`]s, one unit each, and writes them back
//! as the Markdown they carry.
//!
//! [`Extensions`] is what an editor is made with: the hosts' extensions, and the containers
//! callouts and toggles are drawn in (see [`crate::surfaces`]).

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use gpui::{AnyElement, App, Pixels, SharedString, Window};
use markdown::mdast::Node;

/// A host's block or inline piece in the document.
#[derive(Clone)]
pub struct CustomNode {
    name: SharedString,
    data: Arc<dyn Any + Send + Sync>,
    text: SharedString,
    label: SharedString,
    markdown: SharedString,
}

impl fmt::Debug for CustomNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomNode")
            .field("name", &self.name)
            .field("text", &self.text)
            .field("markdown", &self.markdown)
            .finish()
    }
}

impl PartialEq for CustomNode {
    /// Nodes are equal when their name, text, label and Markdown are: the data they draw with
    /// isn't compared.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.text == other.text
            && self.label == other.label
            && self.markdown == other.markdown
    }
}

impl CustomNode {
    /// A node of the extension `name`, holding `data` for drawing it.
    pub fn new<T: Any + Send + Sync>(name: impl Into<SharedString>, data: T) -> Self {
        Self {
            name: name.into(),
            data: Arc::new(data),
            text: SharedString::default(),
            label: SharedString::default(),
            markdown: SharedString::default(),
        }
    }

    /// The node as plain text, for copying it as text and for search.
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = text.into();
        self
    }

    /// What a screen reader says for it.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// The Markdown it's written back as.
    pub fn markdown(mut self, markdown: impl Into<SharedString>) -> Self {
        self.markdown = markdown.into();
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn as_text(&self) -> &str {
        &self.text
    }

    pub fn as_markdown(&self) -> &str {
        &self.markdown
    }

    /// The data the node was made with, when it's a `T`.
    pub fn data<T: Any>(&self) -> Option<&T> {
        self.data.downcast_ref::<T>()
    }
}

/// The text around an inline node, for sizing it to the line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineContext {
    pub font_size: Pixels,
    pub line_height: Pixels,
}

/// A host's kind of block or inline piece.
pub trait Extension: Send + Sync + 'static {
    /// Its name, unique among an editor's extensions.
    fn name(&self) -> &str;

    /// Whether it reads blocks (a paragraph, a fenced code block) rather than inline pieces (a
    /// link).
    fn is_block(&self) -> bool {
        false
    }

    /// The node for this piece of the syntax tree, or `None` to leave it to the editor.
    fn parse(&self, node: &Node) -> Option<CustomNode>;

    /// Draws a block node. Called each time the document is drawn.
    fn render(&self, node: &CustomNode, window: &mut Window, cx: &mut App) -> AnyElement {
        let _ = (window, cx);
        gpui::IntoElement::into_any_element(node.as_text().to_string())
    }

    /// Draws an inline node within its line, with the distance from its top to its text's
    /// baseline, or `None` to draw its text.
    fn render_inline(
        &self,
        node: &CustomNode,
        line: InlineContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, Pixels)> {
        let _ = (node, line, window, cx);
        None
    }
}

/// What an editor parses and draws with: its hosts' extensions, and the containers callouts
/// and toggles are drawn in.
#[derive(Clone)]
pub struct Extensions {
    list: Vec<Arc<dyn Extension>>,
    /// Whether toggles and GitHub's five callouts have their containers. Without them a callout
    /// marker is read as the text of a block quote.
    containers: bool,
}

impl Default for Extensions {
    /// No extensions, and the default containers for toggles and GitHub's five callouts.
    fn default() -> Self {
        Self {
            list: Vec::new(),
            containers: true,
        }
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.list.iter().map(|e| e.name().to_string()))
            .finish()
    }
}

impl Extensions {
    /// No extensions and no containers: callout markers stay the text of block quotes.
    pub fn bare() -> Self {
        Self {
            list: Vec::new(),
            containers: false,
        }
    }

    /// Whether `name`, as written in a marker (`NOTE`), is a callout these extensions draw.
    pub fn is_callout(&self, name: &str) -> bool {
        self.containers
            && crate::import::is_callout_name(name)
            && crate::surfaces::ALERT_NAMES
                .iter()
                .any(|n| n.eq_ignore_ascii_case(name))
    }

    /// Adds an extension. One registered earlier is asked first.
    pub fn with(mut self, extension: impl Extension) -> Self {
        self.list.push(Arc::new(extension));
        self
    }

    /// Adds a shared extension.
    pub fn with_arc(mut self, extension: Arc<dyn Extension>) -> Self {
        self.list.push(extension);
        self
    }

    /// The extensions, in the order they're asked.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Extension>> {
        self.list.iter()
    }
}
