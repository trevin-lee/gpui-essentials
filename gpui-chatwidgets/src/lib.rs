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
//!
//! The README has a complete host, compiled as a doctest.

pub mod openui;

#[cfg(feature = "gpui")]
mod draw;
#[cfg(feature = "gpui")]
mod host;
#[cfg(feature = "gpui")]
mod theme;
#[cfg(feature = "gpui")]
mod view;

#[cfg(feature = "gpui")]
pub use host::{CardEvent, CardHost, ImageLoad, ImageReply, Limits};
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

/// The asset path of a Lucide icon in GPUI Kit's bundled catalog ("icons/calendar.svg"), or
/// None when the catalog hasn't one by that name. For a host that registers GPUI Kit's
/// `AllAssets`, as its [`CardHost::icon`]. The name is only ever looked up in the catalog, never
/// used as a path itself.
#[cfg(feature = "gpui")]
pub fn lucide_icon(name: &str) -> Option<gpui_kit::SharedString> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return None;
    }
    let want = format!("icons/{name}.svg");
    gpui_kit::assets::IconName::ALL
        .iter()
        .map(|i| i.path())
        .find(|p| p.as_ref() == want)
}

#[cfg(feature = "gpui")]
impl gpui_kit::Render for CardView {
    fn render(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        let theme = self.host.theme(cx);
        self.prepare(window, cx);
        draw::card(self, &theme, window, cx)
    }
}
