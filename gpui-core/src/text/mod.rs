//! Text: Markdown drawn natively ([`TextView`]), on a paragraph layer that lays styled text out
//! and answers where things are in it.

mod markdown;
pub mod paragraph;
mod view;

pub use markdown::{Align, Block, Item, parse as parse_markdown};
pub use view::{
    BlockGap, BlockKind, CodeBlock, HeadingMargins, InlineCodeStyle, TextView, TextViewColors,
    TextViewStyle,
};

/// A piece of code's highlighted ranges (bytes, on character boundaries, in order, none
/// overlapping) and their styles (colour, weight and italic).
pub type Highlights = Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>;

/// Highlights code in a language (a code fence's info), as the host provides: gpui-core's
/// `highlight` feature makes one (`Highlighter::into_fn`), or a host brings its own.
pub type HighlightFn = std::sync::Arc<dyn Fn(&str, &str) -> Highlights + Send + Sync + 'static>;

pub(crate) fn init(cx: &mut gpui::App) {
    view::init(cx);
}
