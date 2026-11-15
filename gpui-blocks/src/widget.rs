//! Widget blocks: blocks an app draws itself, kept in Markdown as fenced code.
//!
//! A widget is a fenced block whose info string is the widget's name:
//!
//! ````markdown
//! ```chart
//! kind: bar
//! values: 3, 5, 2
//! ```
//! ````
//!
//! Every Markdown tool keeps a fenced block as it is, and shows it as a code block, so a document
//! with widgets stays readable anywhere. In the editor a widget is one unit, like a picture: it's
//! selected, moved and deleted whole, and written back with its name, the rest of its info
//! string and its body as they were read. What the body means is the widget's business; the
//! editor only hands it over.
//!
//! Register widgets with [`Options::widget`](crate::Options::widget), and give each a slash menu
//! item that inserts it (a [`SlashAction::Markdown`](crate::slash::SlashAction::Markdown) of
//! [`fenced`]).

use std::sync::Arc;

use gpui_kit::base::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin};
use gpui_kit::{AnyElement, App, IntoElement, Window};
use markdown::mdast::Node;

/// A block an app draws itself.
pub trait Widget: Send + Sync + 'static {
    /// The fenced block's info string that marks this widget, such as `chart`.
    fn name(&self) -> &str;

    /// Draws the block from its body, the text between the fences. Called each time the document
    /// is drawn; keep it cheap, and keep any state the widget needs outside it.
    fn render(&self, body: &str, window: &mut Window, cx: &mut App) -> AnyElement;

    /// What a screen reader says for the block.
    fn label(&self, _body: &str) -> String {
        format!("{} block", self.name())
    }
}

/// The Markdown of a widget block: `body` fenced with the widget's name, the fence longer than
/// any run of backticks in the body so the body can't close it.
pub fn fenced(name: &str, body: &str) -> String {
    fenced_with_info(name, body)
}

/// `fenced` with a whole info string: the name and anything written after it.
fn fenced_with_info(info: &str, body: &str) -> String {
    let longest = body.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let body = body.strip_suffix('\n').unwrap_or(body);
    if body.is_empty() {
        format!("{fence}{info}\n{fence}")
    } else {
        format!("{fence}{info}\n{body}\n{fence}")
    }
}

/// A widget as a Markdown plugin: it takes the fenced blocks with its name.
pub(crate) struct WidgetPlugin(pub(crate) Arc<dyn Widget>);

impl MarkdownPlugin for WidgetPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        self.0.name()
    }

    fn parse(&self, node: &Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Code(code) = node else {
            return None;
        };
        if code.lang.as_deref() != Some(self.0.name()) {
            return None;
        }
        // Written back from its parts rather than its source span: inside a quote, a callout or
        // a list the span holds the container's `> ` or indent on each line, which the
        // container adds again when the document is written.
        let body = code.value.replace("\r\n", "\n");
        let info = match code.meta.as_deref().map(str::trim) {
            Some(meta) if !meta.is_empty() => format!("{} {meta}", self.0.name()),
            _ => self.0.name().to_string(),
        };
        let source = fenced_with_info(&info, &body);
        let label = self.0.label(&body);
        Some(
            MarkdownNode::new(self.0.name().to_string(), body)
                .text(label.clone())
                .accessibility_label(label)
                .markdown(source),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let body = node
            .data::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        self.0.render(body, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::fenced;

    #[test]
    fn fenced_blocks_outrun_the_bodys_own_fences() {
        assert_eq!(fenced("chart", "a: 1\n"), "```chart\na: 1\n```");
        assert_eq!(fenced("chart", ""), "```chart\n```");
        assert_eq!(
            fenced("note", "```rust\nfn main() {}\n```"),
            "````note\n```rust\nfn main() {}\n```\n````"
        );
    }
}
