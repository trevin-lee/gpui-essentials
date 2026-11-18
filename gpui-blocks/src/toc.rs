//! A table of contents block: the document's headings, kept up to date as it's edited. A click
//! on one moves the caret to that heading and scrolls to it.
//!
//! In Markdown it's a line holding only `[TOC]`, which many Markdown tools draw as a table of
//! contents too, and the rest show as the text `[TOC]`.

use std::sync::{Arc, Mutex};

use gpui::*;
use gpui_core::ActiveTheme as _;
use markdown::mdast::Node;
use markdown_editor::BlockId;
use markdown_editor::Control as _;
use markdown_editor::{CustomNode, Extension};

/// The block's Markdown.
pub const MARKDOWN: &str = "[TOC]";
/// The custom node's name.
pub(crate) const NAME: &str = "toc";

/// A heading as the table lists it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Heading {
    pub(crate) level: u8,
    pub(crate) text: SharedString,
    pub(crate) block: BlockId,
}

/// Called with a heading's block when its entry is clicked.
pub(crate) type OnHeadingClick = Arc<dyn Fn(BlockId, &mut Window, &mut App) + Send + Sync>;

/// The headings to list, which the editor keeps up to date, and what a click does.
#[derive(Clone, Default)]
pub(crate) struct TocHooks {
    pub(crate) headings: Arc<Mutex<Vec<Heading>>>,
    pub(crate) on_click: Option<OnHeadingClick>,
}

pub(crate) struct TocPlugin(pub(crate) TocHooks);

impl Extension for TocPlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        NAME
    }

    fn parse(&self, node: &Node) -> Option<CustomNode> {
        let Node::Paragraph(paragraph) = node else {
            return None;
        };
        let mut text = String::new();
        for child in &paragraph.children {
            let Node::Text(t) = child else {
                return None;
            };
            text.push_str(&t.value);
        }
        if !text.trim().eq_ignore_ascii_case(MARKDOWN) {
            return None;
        }
        Some(
            CustomNode::new(NAME, ())
                .text("Table of contents")
                .accessibility_label("Table of contents")
                .markdown(MARKDOWN),
        )
    }

    fn render(&self, _: &CustomNode, _: &mut Window, cx: &mut App) -> AnyElement {
        let headings = self
            .0
            .headings
            .lock()
            .map(|h| h.clone())
            .unwrap_or_default();
        let theme = cx.theme();
        let (muted, fg, border) = (theme.muted_foreground, theme.foreground, theme.border);
        let top = headings.iter().map(|h| h.level).min().unwrap_or(1);
        let entries = headings.into_iter().enumerate().map(|(ix, heading)| {
            let on_click = self.0.on_click.clone();
            let block = heading.block;
            div()
                .id(("blocks-toc", ix))
                .debug_selector(move || format!("blocks-toc-{ix}"))
                .control(module_path!(), Role::Link, heading.text.clone())
                .pl(px(16. * (heading.level - top) as f32))
                .py(px(2.))
                // A row, so the underline is as wide as the title.
                .flex()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(muted)
                .cursor_pointer()
                .hover(move |s| s.text_color(fg))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .border_b_1()
                        .border_color(border)
                        .child(heading.text),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, window, cx| {
                    if let Some(on_click) = &on_click {
                        on_click(block, window, cx);
                    }
                })
                .into_any_element()
        });
        let mut list = div()
            .w_full()
            .my(px(4.))
            .flex()
            .flex_col()
            .text_size(px(14.));
        list = list.children(entries);
        if self.0.headings.lock().is_ok_and(|h| h.is_empty()) {
            list = list.child(
                div()
                    .text_color(muted)
                    .child("Headings will be listed here."),
            );
        }
        list.debug_selector(|| "blocks-toc".into())
            .into_any_element()
    }
}
