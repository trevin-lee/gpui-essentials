//! The look every component draws with, given by the host.
//!
//! The colour names are the web app's CSS variables (`globals.css`) in snake case, the names
//! the design tokens use, so a host passes its tokens straight through. Sizes are in rems: the
//! window's rem is the theme's [`Theme::font_size`] (see [`crate::Root`]), so changing that one
//! number zooms everything, as a browser zooms CSS pixels.

use std::ops::{Deref, DerefMut};
use std::sync::LazyLock;

use gpui::{App, Global, Hsla, Pixels, Rems, SharedString, px, rems, rgb, rgba};

/// Light or dark: which way the colours run. Components never pick colours by mode; it's for
/// hosts and for images that need a dark or a light variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThemeMode {
    Light,
    #[default]
    Dark,
}

impl ThemeMode {
    pub fn is_dark(self) -> bool {
        self == ThemeMode::Dark
    }
}

/// Every colour token. Each `x_foreground` is the text drawn on `x`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    /// The page.
    pub background: Hsla,
    /// Text on the page.
    pub foreground: Hsla,
    /// A raised panel on the page.
    pub card: Hsla,
    pub card_foreground: Hsla,
    /// What floats over the page: menus, tooltips, pickers.
    pub popover: Hsla,
    pub popover_foreground: Hsla,
    /// The strongest action.
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    /// A quieter action.
    pub secondary: Hsla,
    pub secondary_foreground: Hsla,
    /// A quiet fill: a track, a well.
    pub muted: Hsla,
    /// Secondary text.
    pub muted_foreground: Hsla,
    /// What's under the pointer or the current item: usually a translucent wash.
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    /// Deleting, blocked, invalid.
    pub destructive: Hsla,
    /// Done, working.
    pub success: Hsla,
    pub border: Hsla,
    /// A field's border, and a switch's track when off.
    pub input: Hsla,
    /// The focus ring (drawn at half strength) and the text selection (at 30%).
    pub ring: Hsla,
    pub sidebar: Hsla,
    pub sidebar_foreground: Hsla,
    pub sidebar_primary: Hsla,
    pub sidebar_primary_foreground: Hsla,
    pub sidebar_accent: Hsla,
    pub sidebar_accent_foreground: Hsla,
    pub sidebar_border: Hsla,
    pub sidebar_ring: Hsla,
    /// An agent working.
    pub live: Hsla,
    /// Something that needs the owner.
    pub attention: Hsla,
    pub attention_foreground: Hsla,
    /// Ready to go.
    pub ready: Hsla,
    /// A chat bubble.
    pub bubble: Hsla,
    /// The faint pattern behind an empty state.
    pub empty_pattern: Hsla,
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn ca(hex: u32) -> Hsla {
    rgba(hex).into()
}

impl Colors {
    /// The web app's `.dark` colours: charcoal and near-black.
    pub fn dark() -> Self {
        Self {
            background: c(0x0f0f11),
            foreground: c(0xededef),
            card: c(0x18181b),
            card_foreground: c(0xededef),
            popover: c(0x18181b),
            popover_foreground: c(0xededef),
            primary: c(0xededef),
            primary_foreground: c(0x0f0f11),
            secondary: c(0x1f1f23),
            secondary_foreground: c(0xededef),
            muted: c(0x1f1f23),
            muted_foreground: c(0x8b8b93),
            accent: ca(0xffffff0f),
            accent_foreground: c(0xededef),
            destructive: c(0xe5665b),
            success: c(0x4cc38a),
            border: c(0x28282d),
            input: c(0x3a3a41),
            ring: c(0xededef),
            sidebar: c(0x0b0b0d),
            sidebar_foreground: c(0xededef),
            sidebar_primary: c(0xededef),
            sidebar_primary_foreground: c(0x0f0f11),
            sidebar_accent: ca(0xffffff0f),
            sidebar_accent_foreground: c(0xededef),
            sidebar_border: c(0x28282d),
            sidebar_ring: c(0xededef),
            live: c(0x4cc38a),
            attention: c(0xd9a441),
            attention_foreground: c(0xe3b45a),
            ready: c(0x4cc38a),
            bubble: c(0x2a2a30),
            empty_pattern: ca(0xffffff0f),
        }
    }

    /// The web app's light colours.
    pub fn light() -> Self {
        Self {
            background: c(0xfafafa),
            foreground: c(0x0f0f11),
            card: c(0xffffff),
            card_foreground: c(0x0f0f11),
            popover: c(0xffffff),
            popover_foreground: c(0x0f0f11),
            primary: c(0x0f0f11),
            primary_foreground: c(0xffffff),
            secondary: c(0xf1f1f3),
            secondary_foreground: c(0x0f0f11),
            muted: c(0xf1f1f3),
            muted_foreground: c(0x6b6b73),
            accent: ca(0x0000000d),
            accent_foreground: c(0x0f0f11),
            destructive: c(0xc7362b),
            success: c(0x1f8f5c),
            border: c(0xe4e4e7),
            input: c(0xd9d9de),
            ring: c(0x0f0f11),
            sidebar: c(0xf1f1f3),
            sidebar_foreground: c(0x0f0f11),
            sidebar_primary: c(0x0f0f11),
            sidebar_primary_foreground: c(0xffffff),
            sidebar_accent: ca(0x0000000d),
            sidebar_accent_foreground: c(0x0f0f11),
            sidebar_border: c(0xe4e4e7),
            sidebar_ring: c(0x0f0f11),
            live: c(0x1f8f5c),
            attention: c(0xd9a441),
            attention_foreground: c(0xa8701a),
            ready: c(0x1f8f5c),
            bubble: c(0xf1f1f3),
            empty_pattern: ca(0x00000012),
        }
    }

    /// The focus ring's colour: the ring at half strength, as the web draws `outline-ring/50`.
    pub fn focus_ring(&self) -> Hsla {
        self.ring.opacity(0.5)
    }

    /// Selected text's highlight: the ring at 30%.
    pub fn selection(&self) -> Hsla {
        self.ring.opacity(0.3)
    }
}

/// The type scale, in rems of the base text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeScale {
    pub xs: Rems,
    pub sm: Rems,
    pub base: Rems,
    pub lg: Rems,
    pub xl: Rems,
}

impl Default for TypeScale {
    /// The web's: 11.25, 13.125, 15, 16.875 and 18.75 CSS pixels on a 15 pixel base.
    fn default() -> Self {
        Self {
            xs: rems(0.75),
            sm: rems(0.875),
            base: rems(1.),
            lg: rems(1.125),
            xl: rems(1.25),
        }
    }
}

/// The theme: colours, fonts and sizes. The host sets it ([`Theme::set`]); components read it
/// with [`ActiveTheme::theme`]. It derefs to its [`Colors`], so `cx.theme().border` works.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub mode: ThemeMode,
    pub colors: Colors,
    /// The family every component's text is set in.
    pub font_family: SharedString,
    /// Code and other fixed-width text.
    pub mono_font_family: SharedString,
    /// The base text size in the window's pixels, zoom included. [`crate::Root`] makes it the
    /// window's rem, which every other size is measured in.
    pub font_size: Pixels,
    pub text: TypeScale,
    /// The base corner radius (the web's `--radius`, its `rounded-lg`), in rems.
    pub radius: Rems,
    /// The smallest a pressable target is drawn in the touch size ([`crate::Size::Touch`]):
    /// 44 points, Apple's minimum. In the window's points; it doesn't zoom.
    pub touch_target: Pixels,
}

impl Theme {
    /// The Mac app's look: the web's dark colours on a 15 pixel base. Its fonts are the
    /// system's (San Francisco, and Menlo for code), which every Mac and iPhone has; a host that
    /// bundles the app's fonts sets `font_family` to "Inter" and `mono_font_family` to
    /// "Geist Mono" after loading them. A family that isn't loaded draws no text at all.
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            colors: Colors::dark(),
            font_family: ".SystemUIFont".into(),
            mono_font_family: "Menlo".into(),
            font_size: px(15.),
            text: TypeScale::default(),
            radius: rems(10. / 15.),
            touch_target: px(44.),
        }
    }

    /// The same with the web's light colours.
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            colors: Colors::light(),
            ..Self::dark()
        }
    }

    /// `rounded-sm`: 0.6 of the radius.
    pub fn radius_sm(&self) -> Rems {
        rems(self.radius.0 * 0.6)
    }

    /// `rounded-md`: 0.8 of the radius.
    pub fn radius_md(&self) -> Rems {
        rems(self.radius.0 * 0.8)
    }

    /// `rounded-lg`: the radius.
    pub fn radius_lg(&self) -> Rems {
        self.radius
    }

    /// `rounded-xl`: 1.4 times the radius.
    pub fn radius_xl(&self) -> Rems {
        rems(self.radius.0 * 1.4)
    }

    /// Makes `theme` the app's theme. Windows pick it up on their next draw; call
    /// `cx.refresh_windows()` to redraw them now.
    pub fn set(theme: Theme, cx: &mut App) {
        cx.set_global(theme);
    }

    /// The app's theme: the one the host set, or [`Theme::dark`] until it sets one.
    pub fn global(cx: &App) -> &Theme {
        cx.try_global::<Theme>().unwrap_or(&DEFAULT)
    }

    /// The app's theme, to change in place (a zoom, a new accent).
    pub fn global_mut(cx: &mut App) -> &mut Theme {
        if !cx.has_global::<Theme>() {
            cx.set_global(Theme::dark());
        }
        cx.global_mut::<Theme>()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

static DEFAULT: LazyLock<Theme> = LazyLock::new(Theme::dark);

impl Global for Theme {}

impl Deref for Theme {
    type Target = Colors;
    fn deref(&self) -> &Colors {
        &self.colors
    }
}

impl DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Colors {
        &mut self.colors
    }
}

/// `cx.theme()` anywhere there's an [`App`] (a `Context` derefs to one).
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radii_follow_the_web() {
        let t = Theme::dark();
        // 10, 6, 8 and 14 CSS pixels on a 15 pixel rem.
        let css = |r: Rems| (r.0 * 15. * 1000.).round() / 1000.;
        assert_eq!(css(t.radius_lg()), 10.);
        assert_eq!(css(t.radius_sm()), 6.);
        assert_eq!(css(t.radius_md()), 8.);
        assert_eq!(css(t.radius_xl()), 14.);
    }

    #[test]
    fn colors_are_reachable_through_the_theme() {
        let t = Theme::light();
        assert_eq!(t.border, Colors::light().border);
        assert_eq!(t.mode, ThemeMode::Light);
        assert!(Theme::dark().mode.is_dark());
        assert_eq!(t.focus_ring().a, 0.5);
    }
}
