//! Draws AI agents' OpenUI cards natively in GPUI.
//!
//! OpenUI (openui.com) is a small language an agent writes to answer with UI (cards, tables,
//! charts, forms, buttons) instead of text. This crate reads it ([`openui`]) and draws it
//! ([`CardView`]).
//!
//! - [`openui`] is the parser and the typed card model. It has no GPUI in it: with
//!   `default-features = false` it's all that builds.
//! - [`CardView`] (the default `gpui` feature) is a GPUI entity that draws one card in the
//!   host's [`CardTheme`], asks the host for images, and hands every press back to it through
//!   [`CardHost`]: the crate makes no network request and opens nothing itself.
//! - [`StreamingMarkdown`] (the `gpui` feature) draws a reply in Markdown as it streams in, a
//!   delta at a time, laying out each finished block once.
//!
//! The README has a complete host, compiled as a doctest.

pub mod openui;

#[cfg(feature = "gpui")]
mod draw;
#[cfg(feature = "gpui")]
mod host;
#[cfg(feature = "gpui")]
mod stream;
#[cfg(feature = "gpui")]
mod theme;
#[cfg(feature = "gpui")]
mod view;

#[cfg(feature = "gpui")]
pub use host::{CardEvent, CardHost, ImageLoad, ImageReply, Limits};
#[cfg(feature = "gpui")]
pub use stream::{StreamStats, StreamingMarkdown};
#[cfg(feature = "gpui")]
pub use theme::{
    CardColors, CardFonts, CardTheme, CodeColors, Radii, Spacing, TextSizes, ToneColors,
};
#[cfg(feature = "gpui")]
pub use view::CardView;

// The README's examples, compiled as doctests.
#[cfg(all(doctest, feature = "gpui"))]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// The asset path of a Lucide icon in gpui-core's bundled set ("icons/calendar.svg"), or None
/// when the set hasn't one by that name. For a host that serves gpui-core's `Icons` asset
/// source, as its [`CardHost::icon`]. The name is only ever looked up in the set, never used as
/// a path itself.
#[cfg(feature = "gpui")]
pub fn lucide_icon(name: &str) -> Option<gpui::SharedString> {
    gpui_core::IconName::named(name).map(|i| i.path())
}

#[cfg(feature = "gpui")]
impl gpui::Render for CardView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let theme = self.host.theme(cx);
        self.prepare(window, cx);
        draw::card(self, &theme, window, cx)
    }
}
