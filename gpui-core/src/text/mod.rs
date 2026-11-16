//! Text: Markdown drawn natively ([`TextView`]), on a paragraph layer that lays styled text out
//! and answers where things are in it.

mod markdown;
pub mod paragraph;
mod view;

pub use markdown::{Align, Block, Item, parse as parse_markdown};
pub use view::{TextView, TextViewStyle};

pub(crate) fn init(cx: &mut gpui::App) {
    view::init(cx);
}
