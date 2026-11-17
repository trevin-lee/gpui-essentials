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

mod a11y;
mod alert;
mod badge;
mod breadcrumb;
mod button;
mod card;
mod collapsible;
mod date_picker;
#[cfg(feature = "highlight")]
pub mod highlight;
pub mod icon;
pub mod input;
mod item;
mod kbd;
mod menu;
mod navigation_menu;
mod picker;
mod popover;
mod press;
mod root;
mod segmented;
mod separator;
mod sheet;
mod size;
mod skeleton;
mod slider;
mod spinner;
mod split_view;
mod switch;
mod tabs;
pub mod text;
pub mod theme;
mod toast;
mod tooltip;

pub use alert::{Alert, AlertVariant};
pub use badge::{Badge, BadgeVariant};
pub use breadcrumb::{Breadcrumb, BreadcrumbItem};
pub use button::{Button, ButtonGroup, ButtonVariant, ButtonVariants};
pub use card::Card;
pub use collapsible::Collapsible;
pub use date_picker::{
    DatePicker, DatePickerColors, DatePickerEvent, DatePickerState, DateValue, MonthGrid,
};
pub use icon::{Icon, IconName, Icons};
pub use input::{Input, InputEvent, InputState};
pub use item::{Item, ItemGroup, ItemSeparator, ItemVariant};
pub use kbd::{Kbd, KbdGroup};
pub use menu::{MenuSelect, PopupMenu, PopupMenuColors, PopupMenuItem, SUBMENU_DELAY};
pub use navigation_menu::{NavigationMenu, NavigationMenuItem, NavigationMenuLink};
pub use picker::Picker;
#[doc(hidden)]
pub use popover::claim_count;
pub use popover::{DismissReason, Popover, PopoverAnchor, PopoverColors};
pub use press::{Press, focus_ring};
pub use root::{FocusNext, FocusPrev, Root};
pub use segmented::SegmentedControl;
pub use separator::Separator;
pub use sheet::Sheet;
pub use size::{ControlSize, Disableable, Selectable, Sizable};
pub use skeleton::Skeleton;
pub use slider::{Slider, SliderEvent, SliderState};
pub use spinner::Spinner;
pub use split_view::{Pane, SplitView};
pub use switch::Switch;
pub use tabs::{Tab, Tabs, TabsOverflow, TabsVariant};
pub use text::{TextView, TextViewStyle};
pub use theme::{ActiveTheme, Colors, Theme, ThemeMode, TypeScale};
pub use toast::{TOAST_DURATION, TOAST_DURATION_LONG, Toast, ToastKind};
pub use tooltip::Tooltip;

use gpui::{AnyWindowHandle, App, AppContext as _, Entity, Render, Window, WindowOptions};

/// Sets the crate up: the default theme if the host has set none, and the key bindings.
///
/// It runs by itself the first time a [`Root`] or an [`InputState`] is made, or a [`TextView`]
/// drawn, so a host that
/// embeds gpui-core's components without calling it still gets their keys; calling it again does
/// nothing.
pub fn init(cx: &mut App) {
    if cx.has_global::<Initialized>() {
        return;
    }
    cx.set_global(Initialized);
    if !cx.has_global::<Theme>() {
        Theme::set(Theme::dark(), cx);
    }
    root::init(cx);
    segmented::init(cx);
    split_view::init(cx);
    tabs::init(cx);
    date_picker::init(cx);
    input::init(cx);
    menu::init(cx);
    navigation_menu::init(cx);
    picker::init(cx);
    popover::init(cx);
    text::init(cx);
}

/// Runs [`init`] after this frame if it hasn't run: for a component with no state of its own to
/// run it from (keys can't be bound in the middle of a draw).
pub(crate) fn init_soon(cx: &mut App) {
    if !cx.has_global::<Initialized>() {
        cx.defer(init);
    }
}

/// That [`init`] has run.
struct Initialized;

impl gpui::Global for Initialized {}

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
