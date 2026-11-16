//! What a card needs from the app it's in, and what it hands back: the [`CardHost`] trait
//! (theme, image bytes, icons, and where a press's message or link goes), [`CardEvent`]s for
//! anyone subscribed, and the [`Limits`] the renderer holds every card to.

use std::sync::Arc;

use gpui_kit::{App, Image, SharedString, WeakEntity};

use crate::CardTheme;
use crate::openui::{ImageSrc, Link, Store};
use crate::view::CardView;

/// The app around a card. The card calls these, never anything else: it makes no network
/// request, opens nothing and sends nothing itself.
///
/// Every call comes after the card's own update has finished (deferred), so an implementation
/// may update the card, or anything else, from inside it.
pub trait CardHost: 'static {
    /// The card's look. Called on every draw, so a host whose look changes (light and dark,
    /// zoom) returns the current one and calls [`CardView::refresh`].
    fn theme(&self, cx: &App) -> CardTheme;

    /// Send `text` to the agent as the user's message: what a press with @ToAssistant (or a
    /// button without an action) says, with any context and the form's values as JSON below
    /// the words, as OpenUI's web renderer writes it.
    fn send(&self, text: &str, cx: &mut App);

    /// Open a link the user pressed (@OpenUrl, or a link in Markdown). It's always http or
    /// https with no user name or password in it; anything else is dropped before this.
    fn open_link(&self, link: &Link, cx: &mut App);

    /// The bytes of a card's image, by the reference the card wrote ([`ImageSrc::as_str`]).
    /// Return [`ImageLoad::Ready`] at once, or [`ImageLoad::Pending`] and later
    /// [`ImageReply::deliver`]; [`ImageLoad::Refused`] (the default) shows the image's alt
    /// text. The host decides which references it serves: [`ImageSrc::same_origin`] helps
    /// with the common case of images from the host's own server. Asked once per reference
    /// per card.
    fn image(&self, src: &ImageSrc, reply: ImageReply, cx: &mut App) -> ImageLoad {
        let _ = (src, reply, cx);
        ImageLoad::Refused
    }

    /// The asset path of an SVG icon for a Lucide icon name a card wrote ("calendar",
    /// "circle-check"), or None to draw none. [`lucide_icon`](crate::lucide_icon) finds names
    /// in gpui-core's bundled Lucide set, for a host that serves gpui-core's `Icons`.
    fn icon(&self, name: &str, cx: &App) -> Option<SharedString> {
        let _ = (name, cx);
        None
    }

    /// Called while drawing for each region that scrolls its content sideways (a wide table,
    /// a code block, a carousel) or clips its own text (an input), with the debug selector the
    /// region carries. A host's overflow test can leave the glyphs inside those bounds alone.
    fn scroller(&self, selector: &str) {
        let _ = selector;
    }

    /// The card's state changed (a field edited, a $variable set by a press). A host that
    /// keeps cards' state across redraws of its list saves [`CardView::store`] here.
    fn changed(&self, store: &Store, cx: &mut App) {
        let _ = (store, cx);
    }
}

/// An image's bytes, or why there are none yet.
#[derive(Clone)]
pub enum ImageLoad {
    /// The image, encoded (PNG, JPEG, GIF, WebP, SVG...): `Image::from_bytes(format, bytes)`.
    Ready(Arc<Image>),
    /// On its way: the host calls [`ImageReply::deliver`] when it has it.
    Pending,
    /// Not shown: the card draws the image's alt text in its place.
    Refused,
}

/// Where a pending image goes when the host has it.
#[derive(Clone)]
pub struct ImageReply {
    pub(crate) view: WeakEntity<CardView>,
    pub(crate) src: ImageSrc,
}

impl ImageReply {
    /// The reference this reply is for.
    pub fn src(&self) -> &ImageSrc {
        &self.src
    }

    /// Hands the card its image (or None: it couldn't be had, so the alt text stays). Safe to
    /// call from anywhere on the main thread, at any time; a card that's gone ignores it.
    pub fn deliver(self, image: Option<Arc<Image>>, cx: &mut App) {
        cx.defer(move |cx| {
            let _ = self
                .view
                .update(cx, |v, cx| v.image_arrived(self.src, image, cx));
        });
    }
}

/// What a card tells anyone subscribed to it (`cx.subscribe(&card, ...)`), besides its host.
#[derive(Clone, Debug, PartialEq)]
pub enum CardEvent {
    /// Send this to the agent as the user's message (also [`CardHost::send`]).
    Send(String),
    /// Open this link (also [`CardHost::open_link`]).
    Open(Link),
    /// The card's state changed (also [`CardHost::changed`]).
    Changed,
}

/// What one card may draw at most, on top of the parser's own caps (the `MAX_` constants in
/// [`openui`](crate::openui)). Past these, a note says what was left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Rows of a table drawn; the rest are counted in a note under it.
    pub table_rows: usize,
    /// Rows of an editable table drawn.
    pub editable_rows: usize,
    /// Points per series a chart may have; a bigger chart shows a note instead.
    pub chart_points: usize,
    /// Series a chart may have.
    pub chart_series: usize,
    /// Images a gallery draws.
    pub gallery_images: usize,
    /// Items drawn in one list, tag block, entity list or card block.
    pub list_items: usize,
    /// Characters of one code block drawn.
    pub code_chars: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            table_rows: 200,
            editable_rows: 100,
            chart_points: 400,
            chart_series: 12,
            gallery_images: 24,
            list_items: 100,
            code_chars: 20_000,
        }
    }
}
