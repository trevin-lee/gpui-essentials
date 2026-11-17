//! [`StreamingMarkdown`]: an agent's reply drawn as it's written, a delta at a time.

mod doc;

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::text::{TextView, TextViewStyle};

use doc::{Doc, display, trim_blank_lines};

type OnLink = Rc<dyn Fn(&str, &Modifiers, &mut Window, &mut App)>;

/// An agent's reply in Markdown, drawn as it streams in: the host makes one when a reply starts
/// (a chat server's draft), feeds it the text as it comes ([`append`](Self::append), or
/// [`replace`](Self::replace) with the whole draft), and [`end`](Self::end)s or
/// [`finish`](Self::finish)es it. It sends, fetches and opens nothing: the host brings the text
/// and handles links.
///
/// It draws with one gpui-core [`TextView`] of keyed sources ([`TextView::blocks`]): each block
/// of the Markdown that's finished (a paragraph, a list, a code block, a table) is a source of
/// its own, and the open last block is the last, in the [`TextViewStyle`] and the text colour,
/// size and font of what's around it. It's drawn, spaced, selected and copied as a single
/// TextView of the same text. A finished block keeps its layout; on each delta only the open
/// last block is read and laid out again, so a delta costs about the same at the end of a long
/// reply as at its start.
///
/// While it's being written:
/// - a steady caret sits after the text's last character, in the text's colour. It's drawn, not
///   laid out, and doesn't blink: nothing draws while no text comes.
/// - what's open on the last line is closed for now (`**bold`, `` `code ``), so the words
///   being written show as they'll end up and nothing after them flips and back; a link being
///   written shows as its words, an open code fence as code, and a line of only Markdown marks
///   (`1.`, `#`, a table's first row) waits for its words.
/// - its height only grows: if the open block lays out shorter for a moment, the space is kept.
///
/// Text can be selected across blocks, and a selection in the finished blocks stays while the
/// draft goes on being written.
pub struct StreamingMarkdown {
    id: SharedString,
    style: TextViewStyle,
    selectable: bool,
    on_link: Option<OnLink>,
    doc: Doc,
    writing: bool,
    /// The text changed since the blocks were last worked out.
    dirty: bool,
    /// The open tail as drawn, under its key: empty while the draft is written and nothing open
    /// shows yet, none once it has ended with nothing open.
    tail: Option<(u64, SharedString)>,
    /// The width the content was last laid out at, and the tallest it's been at that width.
    tallest: Rc<Cell<(Pixels, Pixels)>>,
    /// Times the open tail was read again, for [`StreamStats`].
    tail_reads: usize,
}

/// What a [`StreamingMarkdown`] has done, for tests and benchmarks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Blocks settled now.
    pub settled: usize,
    /// Blocks ever made. Each is made once, when it settles, and from then on keeps its key in
    /// the TextView, and so its layout, so this stays at about `settled` however many deltas
    /// come.
    pub made: usize,
    /// Times the open tail was read again (once a frame with new text at most).
    pub tail_reads: usize,
    /// Bytes settling read again to find where blocks end: about twice each settled block's
    /// length, however the text came, as only a line that could end the open block is checked.
    pub reread: usize,
}

impl StreamingMarkdown {
    /// An empty draft, being written. `id` prefixes its TextView's element id: give each its own
    /// (the draft's id).
    pub fn new(id: impl Into<SharedString>) -> StreamingMarkdown {
        StreamingMarkdown {
            id: id.into(),
            style: TextViewStyle::default(),
            selectable: false,
            on_link: None,
            doc: Doc::default(),
            writing: true,
            dirty: true,
            tail: None,
            tallest: Rc::default(),
            tail_reads: 0,
        }
    }

    /// The Markdown's layout: the same style the host draws its finished messages with.
    pub fn style(mut self, style: TextViewStyle) -> StreamingMarkdown {
        self.style = style;
        self
    }

    /// Whether its text can be selected with the pointer and copied, as
    /// [`TextView::selectable`].
    pub fn selectable(mut self, selectable: bool) -> StreamingMarkdown {
        self.selectable = selectable;
        self
    }

    /// Called with a link's address when it's pressed, as [`TextView::on_link_click`]. The
    /// address is what the agent wrote: check it before opening anything.
    pub fn on_link_click(
        mut self,
        f: impl Fn(&str, &Modifiers, &mut Window, &mut App) + 'static,
    ) -> StreamingMarkdown {
        self.on_link = Some(Rc::new(f));
        self
    }

    /// A new style (the host's theme changed): every block lays out again once.
    pub fn set_style(&mut self, style: TextViewStyle, cx: &mut Context<Self>) {
        self.style = style;
        cx.notify();
    }

    /// Adds text to the end of the draft: a delta, Markdown cut anywhere. Ignored once the
    /// draft has ended.
    pub fn append(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.writing || text.is_empty() {
            return;
        }
        self.doc.append(text);
        self.changed(cx);
    }

    /// The whole draft so far, in place of what it had (a reconnect mid-reply). What it shares
    /// with the text before stays laid out. Ignored once the draft has ended.
    pub fn replace(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.writing || text == self.doc.text {
            return;
        }
        self.doc.replace(text);
        self.changed(cx);
    }

    /// The draft is finished: the caret goes and the text stays as it is, Markdown read as
    /// written (what was open stays open, as it would be in a message of the same text).
    pub fn end(&mut self, cx: &mut Context<Self>) {
        if self.writing {
            self.writing = false;
            self.changed(cx);
        }
    }

    /// Ends the draft with its message's text, which may differ from the draft's: a reply
    /// split into several messages begins with one that's shorter, and a message's text can be
    /// tidied. Blocks the two share stay as they are, laid out where they were; only what
    /// differs is read and laid out again. With the same text, nothing moves: the caret goes.
    ///
    /// The height can only shrink here: by what the message leaves out, and by any space kept
    /// while the draft was being written (see [`StreamingMarkdown`]).
    pub fn finish(&mut self, text: &str, cx: &mut Context<Self>) {
        if text != self.doc.text {
            self.doc.replace(text);
        }
        self.writing = false;
        self.changed(cx);
    }

    /// The text so far.
    pub fn text(&self) -> &str {
        &self.doc.text
    }

    /// Whether the draft is still being written (not ended or finished).
    pub fn is_writing(&self) -> bool {
        self.writing
    }

    /// What it has done, for tests and benchmarks.
    #[doc(hidden)]
    pub fn stats(&self) -> StreamStats {
        StreamStats {
            settled: self.doc.blocks.len(),
            made: self.doc.made,
            tail_reads: self.tail_reads,
            reread: self.doc.reread,
        }
    }

    /// Works out the blocks now, as the next frame would, without the open tail: for
    /// benchmarks. True when settling has more to do, which the next frame would carry on.
    #[doc(hidden)]
    pub fn settle_now(&mut self) -> bool {
        if self.writing {
            self.doc.settle()
        } else {
            self.doc.restore()
        }
    }

    /// The open tail as it's drawn now (closed for now), for tests.
    #[doc(hidden)]
    pub fn shown_tail(&self) -> Option<SharedString> {
        self.tail.as_ref().map(|(_, s)| s.clone())
    }

    /// The settled blocks' text, from the start of the draft, for tests.
    #[doc(hidden)]
    pub fn settled_text(&self) -> &str {
        &self.doc.text[..self.doc.settled]
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        cx.notify();
    }

    /// Works out the blocks and the tail after the text changed: once a frame at most, however
    /// many deltas came since the last. Settling reads a bounded amount a frame; true when it
    /// has more to do, in the next frame.
    fn update(&mut self) -> bool {
        if !self.dirty {
            return false;
        }
        let more = if self.writing {
            self.doc.settle()
        } else if self.doc.defines_footnotes() {
            // A footnote defined in another block than its reference: the TextView resolves a
            // link's reference across its sources, but not a footnote's. Read the whole text as
            // one, as a message is.
            self.doc.reopen_from(0);
            false
        } else {
            self.doc.restore()
        };
        self.dirty = more;
        let tail = self.doc.tail();
        let source = if self.writing {
            display(tail)
        } else {
            trim_blank_lines(tail).to_string()
        };
        self.tail_reads += 1;
        // While it's written, an empty tail is still the last source, so the TextView puts the
        // caret after the last character before it (at the start with no text at all). Once
        // ended, nothing open is nothing drawn.
        self.tail = (self.writing || !source.is_empty())
            .then(|| (self.doc.tail_key() as u64, source.into()));
        more
    }
}

impl Render for StreamingMarkdown {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.update() {
            window.request_animation_frame();
        }
        let writing = self.writing;
        let sources = self
            .doc
            .blocks
            .iter()
            .map(|b| (b.key as u64, b.source.clone()))
            .chain(self.tail.clone());
        let mut view = TextView::blocks(ElementId::Name(format!("{}/md", self.id).into()), sources)
            .style(self.style.clone())
            .selectable(self.selectable)
            .trailing_caret(writing.then(|| window.text_style().color));
        if let Some(f) = self.on_link.clone() {
            view = view.on_link_click(move |url, m, window, cx| f(url, m, window, cx));
        }
        // While it's being written its height never goes down: the tallest it's been at this
        // width is kept.
        let (_, tallest) = self.tallest.get();
        let seen = self.tallest.clone();
        let content = div()
            .relative()
            .w_full()
            .min_w_0()
            .child(view)
            .when(writing, |d| {
                d.child(
                    canvas(
                        move |b, _, _| {
                            let (w, h) = seen.get();
                            let h = if w == b.size.width { h } else { px(0.) };
                            seen.set((b.size.width, h.max(b.size.height)));
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            });
        let sel = format!("{}:draft", self.id);
        div()
            .debug_selector(move || sel)
            .w_full()
            .min_w_0()
            .when(writing, |d| d.min_h(tallest))
            .child(content)
    }
}
