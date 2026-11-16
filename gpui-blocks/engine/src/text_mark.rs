// Added in Trevin Lee's fork (2026): the engine's own inline types.
//! How text is marked (bold, a link, a highlight) and the pictures in it, as the editor keeps
//! them in its document.

use gpui::{Hsla, SharedString, SharedUri};

/// Where a link goes.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LinkMark {
    pub url: SharedString,
    /// The label of a reference link (`[text][label]`), when it's written as one.
    pub identifier: Option<SharedString>,
    pub title: Option<SharedString>,
}

/// The marks on a stretch of text.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TextMark {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
    pub code: bool,
    /// A fill behind the text (`<mark>`), or `None`.
    pub highlight: Option<Hsla>,
    pub link: Option<LinkMark>,
}

impl TextMark {
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn code(mut self) -> Self {
        self.code = true;
        self
    }

    /// Highlighted (`<mark>`) in `color`.
    pub fn highlight(mut self, color: Hsla) -> Self {
        self.highlight = Some(color);
        self
    }

    /// Adds `other`'s marks; its highlight and link replace these.
    pub fn merge(&mut self, other: TextMark) {
        self.bold |= other.bold;
        self.italic |= other.italic;
        self.strikethrough |= other.strikethrough;
        self.underline |= other.underline;
        self.code |= other.code;
        if other.highlight.is_some() {
            self.highlight = other.highlight;
        }
        if let Some(link) = other.link {
            self.link = Some(link);
        }
    }
}

/// A picture in the text.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImageNode {
    pub url: SharedUri,
    pub title: Option<SharedString>,
    pub alt: Option<SharedString>,
}
