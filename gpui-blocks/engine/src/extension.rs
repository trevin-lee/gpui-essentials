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
use gpui_kit::base::text::{
    InlineElement, InlineRenderContext, MarkdownExtensions, MarkdownNode, MarkdownParseContext,
    MarkdownPlugin,
};
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

    fn into_markdown_node(self) -> MarkdownNode {
        let (text, label, markdown) =
            (self.text.clone(), self.label.clone(), self.markdown.clone());
        let name = self.name.clone();
        let mut node = MarkdownNode::new(name, self).text(text).markdown(markdown);
        if !label.is_empty() {
            node = node.accessibility_label(label);
        }
        node
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
}

impl Default for Extensions {
    /// No extensions, and the default containers for toggles and GitHub's five callouts.
    fn default() -> Self {
        Self { list: Vec::new() }
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

/// An extension as GPUI Kit's renderer sees it, until the editor parses and draws on its own.
struct Bridge(Arc<dyn Extension>);

impl MarkdownPlugin for Bridge {
    fn is_block(&self) -> bool {
        self.0.is_block()
    }

    fn name(&self) -> &str {
        self.0.name()
    }

    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        self.0.parse(node).map(CustomNode::into_markdown_node)
    }

    fn render(
        &self,
        node: &MarkdownNode,
        window: &mut Window,
        cx: &mut App,
    ) -> impl gpui::IntoElement {
        match node.data::<CustomNode>() {
            Some(custom) => self.0.render(custom, window, cx),
            None => gpui::IntoElement::into_any_element(node.as_text().to_string()),
        }
    }

    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineElement> {
        let custom = node.data::<CustomNode>()?;
        let line = InlineContext {
            font_size: context.font_size(),
            line_height: context.line_height(),
        };
        let (element, baseline) = self.0.render_inline(custom, line, window, cx)?;
        Some(InlineElement::new(element).with_baseline(baseline))
    }
}

impl From<&Extensions> for MarkdownExtensions {
    fn from(extensions: &Extensions) -> Self {
        extensions.list.iter().fold(
            crate::surfaces::with_default_surfaces(MarkdownExtensions::default()),
            |ext, e| ext.plugin(Bridge(e.clone())),
        )
    }
}

impl From<Extensions> for MarkdownExtensions {
    fn from(extensions: Extensions) -> Self {
        (&extensions).into()
    }
}
