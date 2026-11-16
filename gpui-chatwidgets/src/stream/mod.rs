//! [`StreamingMarkdown`]: an agent's reply drawn as it's written, a delta at a time.

mod doc;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::text::{BlockKind, TextView, TextViewStyle, parse_markdown};

use doc::{Doc, display, kinds, trim_blank_lines};

type OnLink = Rc<dyn Fn(&str, &Modifiers, &mut Window, &mut App)>;

/// An agent's reply in Markdown, drawn as it streams in: the host makes one when a reply starts
/// (a chat server's draft), feeds it the text as it comes ([`append`](Self::append), or
/// [`replace`](Self::replace) with the whole draft), and [`end`](Self::end)s or
/// [`finish`](Self::finish)es it. It sends, fetches and opens nothing: the host brings the text
/// and handles links.
///
/// It draws with gpui-core's [`TextView`], one per block of the Markdown (a paragraph, a list,
/// a code block, a table), with the [`TextViewStyle`] and the text colour, size and font of
/// what's around it, as a single TextView of the same text is drawn. A block is laid out once,
/// when it's finished; on each delta only the open last block is read and laid out again, so a
/// delta costs about the same at the end of a long reply as at its start.
///
/// While it's being written:
/// - a steady caret ([`StreamingMarkdown::CARET`]) sits at the end of the text, in the text's colour. It doesn't
///   blink: nothing draws while no text comes.
/// - what's open on the last line is closed for now (`**bold`, `` `code ``), so the words
///   being written show as they'll end up and nothing after them flips and back; a link being
///   written shows as its words, an open code fence as code, and a line of only Markdown marks
///   (`1.`, `#`, a table's first row) waits for its words.
/// - its height only grows: if the open block lays out shorter for a moment, the space is kept.
///
/// Text selection works within a block (a paragraph, a list), not across blocks. While the
/// draft is written, a settled block far off screen (the start of a long reply a chat has
/// scrolled past) is drawn as the space it took, and as text again when it comes near the
/// screen or the draft ends: a selection in it goes then.
pub struct StreamingMarkdown {
    id: SharedString,
    style: Rc<TextViewStyle>,
    selectable: bool,
    on_link: Option<OnLink>,
    doc: Doc,
    writing: bool,
    /// The text changed since the blocks were last worked out.
    dirty: bool,
    tail: Option<Tail>,
    /// The width the content was last laid out at, and the tallest it's been at that width.
    tallest: Rc<Cell<(Pixels, Pixels)>>,
    /// Times the open tail was read again, for [`StreamStats`].
    tail_reads: usize,
    /// Where each settled block was last drawn, by its key.
    seen: HashMap<usize, Rc<Cell<Seen>>>,
    kept_as_space: Cell<usize>,
}

/// Where a block was last drawn: its size, and whether it was on screen or near it.
#[derive(Clone, Copy, Default)]
struct Seen {
    size: Size<Pixels>,
    near: bool,
}

/// The open tail as drawn: its text and the kinds of its first and last blocks.
struct Tail {
    source: SharedString,
    kinds: Option<(BlockKind, BlockKind)>,
}

/// What a [`StreamingMarkdown`] has done, for tests and benchmarks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Blocks settled now.
    pub settled: usize,
    /// Blocks ever made. Each is made once, when it settles, and from then on its view keeps
    /// its layout, so this stays at about `settled` however many deltas come.
    pub made: usize,
    /// Times the open tail was read again (once a frame with new text at most).
    pub tail_reads: usize,
    /// Settled blocks drawn last frame as the space they take, being far off screen.
    pub kept_as_space: usize,
}

impl StreamingMarkdown {
    /// The caret at the end of the text while it's being written: a steady bar, drawn as text
    /// in the text's own colour, so it sits where the next word goes and wraps with it.
    pub const CARET: &'static str = doc::CARET;

    /// An empty draft, being written. `id` prefixes its views' element ids: give each its own
    /// (the draft's id).
    pub fn new(id: impl Into<SharedString>) -> StreamingMarkdown {
        StreamingMarkdown {
            id: id.into(),
            style: Rc::new(TextViewStyle::default()),
            selectable: false,
            on_link: None,
            doc: Doc::default(),
            writing: true,
            dirty: true,
            tail: None,
            tallest: Rc::default(),
            tail_reads: 0,
            seen: HashMap::new(),
            kept_as_space: Cell::new(0),
        }
    }

    /// The Markdown's layout: the same style the host draws its finished messages with.
    pub fn style(mut self, style: TextViewStyle) -> StreamingMarkdown {
        self.style = Rc::new(style);
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
        self.style = Rc::new(style);
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
            kept_as_space: self.kept_as_space.get(),
        }
    }

    /// The open tail as it's drawn now (closed for now, with the caret), for tests.
    #[doc(hidden)]
    pub fn shown_tail(&self) -> Option<SharedString> {
        self.tail.as_ref().map(|t| t.source.clone())
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        cx.notify();
    }

    /// Works out the blocks and the tail after the text changed: once a frame at most, however
    /// many deltas came since the last.
    fn update(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.writing {
            self.doc.settle();
        } else if self.doc.defines_links() {
            // A link defined below a block that settled before the definition came: read the
            // whole text as one, as a message is.
            self.doc.reopen_from(0);
        } else {
            self.doc.restore();
        }
        let tail = self.doc.tail();
        let source = if self.writing {
            display(tail)
        } else {
            trim_blank_lines(tail).to_string()
        };
        self.tail_reads += 1;
        self.tail = (!source.is_empty()).then(|| Tail {
            kinds: kinds(&parse_markdown(&source)),
            source: source.into(),
        });
    }

    fn text_view(&self, key: usize, source: SharedString) -> Div {
        let id = ElementId::Name(format!("{}/md{key}", self.id).into());
        let mut view = TextView::markdown(id, source)
            .style((*self.style).clone())
            .selectable(self.selectable);
        if let Some(f) = self.on_link.clone() {
            view = view.on_link_click(move |url, m, window, cx| f(url, m, window, cx));
        }
        let sel = format!("{}:md{key}", self.id);
        div()
            .debug_selector(move || sel)
            .w_full()
            .min_w_0()
            .child(view)
    }
}

impl Render for StreamingMarkdown {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.update();
        let mut content = div().relative().w_full().min_w_0().flex().flex_col();
        let mut above: Option<BlockKind> = None;
        let keys: Vec<usize> = self.doc.blocks.iter().map(|b| b.key).collect();
        self.seen.retain(|k, _| keys.contains(k));
        for k in keys {
            self.seen.entry(k).or_default();
        }
        let blocks = self
            .doc
            .blocks
            .iter()
            .filter_map(|b| Some((Some(b.key), b.key, b.source.clone(), b.kinds?)));
        let tail = self
            .tail
            .as_ref()
            .and_then(|t| Some((None, self.doc.tail_key(), t.source.clone(), t.kinds?)));
        let mut space = 0;
        for (settled, key, source, (first, last)) in blocks.chain(tail) {
            if let Some(prev) = above {
                content = content.child(Gap {
                    prev,
                    next: first,
                    style: self.style.clone(),
                });
            }
            let seen = settled.and_then(|k| self.seen.get(&k).cloned());
            // While the draft is written, a settled block far off screen (the start of a long
            // reply the chat has scrolled past) is the space it took: building and measuring
            // its view every frame would make each delta cost more the longer the reply.
            let far = seen
                .as_ref()
                .map(|s| s.get())
                .filter(|s| self.writing && !s.near && s.size.height > px(0.));
            let child = match far {
                Some(s) => {
                    space += 1;
                    let sel = format!("{}:md{key}", self.id);
                    div()
                        .debug_selector(move || sel)
                        .w_full()
                        .flex_none()
                        .h(s.size.height)
                        .into_any_element()
                }
                None => self.text_view(key, source).into_any_element(),
            };
            content = content.child(OnScreen {
                child,
                seen,
                space: far.map(|s| s.size.width),
            });
            above = Some(last);
        }
        self.kept_as_space.set(space);
        // While it's being written its height never goes down: the tallest it's been at this
        // width is kept.
        let writing = self.writing;
        let (_, tallest) = self.tallest.get();
        if writing {
            let seen = self.tallest.clone();
            content = content.child(
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
            );
        }
        let sel = format!("{}:draft", self.id);
        div()
            .debug_selector(move || sel)
            .w_full()
            .min_w_0()
            .when(writing, |d| d.min_h(tallest))
            .child(content)
    }
}

/// A block that's painted only where it can be seen: one scrolled out of the chat's view (the
/// start of a long reply, while its end streams in) is laid out, but neither prepainted nor
/// painted, so a frame costs what's on screen rather than the whole reply. It notes where the
/// block was, and a block kept as space that comes near the screen (or whose width changed) is
/// drawn as text again from the next frame.
struct OnScreen {
    child: AnyElement,
    seen: Option<Rc<Cell<Seen>>>,
    /// The width a block kept as space had when it was last drawn as text.
    space: Option<Pixels>,
}

impl IntoElement for OnScreen {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for OnScreen {
    type RequestLayoutState = ();
    type PrepaintState = bool;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let mask = window.content_mask().bounds;
        let on_screen = bounds.intersects(&mask);
        // Near: within a screen's height of what's shown, so a scroll or a flick reaches text
        // that's drawn already.
        let near = bounds.intersects(&mask.dilate(mask.size.height.max(px(200.))));
        if let Some(seen) = &self.seen {
            match self.space {
                Some(width) => {
                    if near || width != bounds.size.width {
                        seen.set(Seen::default());
                        window.request_animation_frame();
                    }
                }
                None => seen.set(Seen {
                    size: bounds.size,
                    near,
                }),
            }
        }
        if on_screen {
            self.child.prepaint(window, cx);
        }
        on_screen
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        on_screen: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *on_screen {
            self.child.paint(window, cx);
        }
    }
}

/// The space between two blocks, as gpui-core's TextView puts it between the same two blocks
/// in one view: the style's `block_gap` for the pair, else the larger of a heading's margins,
/// else the paragraph gap. Worked out where GPUI lays out, in the text style around it, as the
/// TextView's is.
struct Gap {
    prev: BlockKind,
    next: BlockKind,
    style: Rc<TextViewStyle>,
}

impl IntoElement for Gap {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Gap {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let h = gap(
            &self.style,
            self.prev,
            self.next,
            &window.text_style(),
            window.rem_size(),
        );
        let mut style = Style::default();
        style.size.height = h.into();
        style.flex_shrink = 0.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

/// gpui-core's TextView's space between two sibling blocks.
pub(crate) fn gap(
    style: &TextViewStyle,
    prev: BlockKind,
    next: BlockKind,
    text: &TextStyle,
    rem: Pixels,
) -> Pixels {
    if let Some(g) = style.block_gap.as_ref().and_then(|f| f(prev, next)) {
        return g;
    }
    let paragraph = style.paragraph_gap.to_pixels(rem);
    let size = text.font_size.to_pixels(rem);
    let heading_size = |level: u8| {
        let base = style.heading_base_font_size.unwrap_or(size);
        match &style.heading_font_size {
            Some(f) => f(level, base),
            None => {
                base * match level {
                    1 => 1.6,
                    2 => 1.35,
                    3 => 1.15,
                    _ => 1.,
                }
            }
        }
    };
    let margins = |k: BlockKind| match k {
        BlockKind::Heading(level) => Some(match &style.heading_margins {
            Some(f) => f(level, heading_size(level)),
            None => (
                style.heading_margin_top.unwrap_or(paragraph),
                style.heading_margin_bottom.unwrap_or(paragraph),
            ),
        }),
        _ => None,
    };
    match (margins(prev), margins(next)) {
        (Some((_, below)), Some((above, _))) => below.max(above),
        (Some((_, below)), None) => below,
        (None, Some((above, _))) => above,
        (None, None) => paragraph,
    }
}
