//! Components for GPUI apps on macOS and iOS, drawn in the host's theme.
//!
//! GPUI itself is the `gpui` crate (published as `gpui-pre`); depend on it under that name,
//! at the same version as this crate, and use it directly. This crate adds the theme, the
//! window's [`Root`] and the components.
//!
//! ```no_run
//! use gpui::*;
//! use gpui_core::{Theme, open_window};
//!
//! struct Hello;
//!
//! impl Render for Hello {
//!     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!         div().child("Hello")
//!     }
//! }
//!
//! fn run(cx: &mut App) {
//!     gpui_core::init(cx);
//!     Theme::set(Theme::dark(), cx);
//!     open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Hello)).unwrap();
//! }
//! ```
//!
//! Every component follows the same rules: nothing redraws while nothing changes; what moves,
//! moves on GPUI's frame clock; text stays inside its box at any width; everything works from
//! the keyboard and shows a focus ring; nothing works by hover alone; and links, actions and
//! images go to the host as events. The crate never fetches or opens anything itself.

/// The README's example, compiled as a doctest.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

mod root;
mod size;
pub mod theme;

pub use root::{FocusNext, FocusPrev, Root};
pub use size::{Disableable, Selectable, Sizable, Size};
pub use theme::{ActiveTheme, Colors, Theme, ThemeMode, TypeScale};

use gpui::{AnyWindowHandle, App, AppContext as _, Entity, Render, Window, WindowOptions};

/// Sets the crate up: the default theme if the host has set none, and the key bindings.
/// Call once, before opening windows.
pub fn init(cx: &mut App) {
    if !cx.has_global::<Theme>() {
        Theme::set(Theme::dark(), cx);
    }
    root::init(cx);
}

/// Opens a window with `build`'s view inside a [`Root`], and returns the window and the view.
pub fn open_window<V: Render>(
    options: WindowOptions,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> gpui::Result<(AnyWindowHandle, Entity<V>)> {
    let mut built = None;
    let window = cx.open_window(options, |window, cx| {
        let view = build(window, cx);
        built = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    })?;
    Ok((
        window.into(),
        built.expect("open_window ran its build closure"),
    ))
}
