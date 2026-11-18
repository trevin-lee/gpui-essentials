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
mod accordion;
mod alert;
mod aspect_ratio;
mod avatar;
mod badge;
pub mod base;
mod breadcrumb;
mod button;
mod card;
mod carousel;
pub mod chart;
mod checkbox;
mod collapsible;
mod combobox;
mod command;
mod context_menu;
mod date_picker;
mod dialog;
mod drawer;
mod dropdown_menu;
mod empty_state;
mod field;
#[cfg(feature = "highlight")]
pub mod highlight;
mod hover_card;
pub mod icon;
pub mod input;
mod input_group;
mod input_otp;
mod item;
mod kbd;
mod menu;
mod menubar;
mod modal;
mod navigation_menu;
mod pagination;
mod picker;
mod popover;
mod press;
mod progress;
mod radio_group;
mod root;
mod scroll_area;
mod search_list;
mod segmented;
mod select;
mod separator;
mod sheet;
mod sidebar;
mod size;
mod skeleton;
mod slider;
mod spinner;
mod split_view;
mod switch;
mod table;
mod tabs;
pub mod text;
pub mod theme;
mod toast;
mod toggle;
mod tooltip;

#[doc(hidden)]
pub use a11y::{Audited, audit, audit_start, audit_take};
pub use accordion::{Accordion, AccordionItem};
pub use alert::{Alert, AlertVariant};
pub use aspect_ratio::AspectRatio;
pub use avatar::{Avatar, AvatarGroup, AvatarImage};
pub use badge::{Badge, BadgeVariant};
pub use breadcrumb::{Breadcrumb, BreadcrumbItem};
pub use button::{Button, ButtonGroup, ButtonVariant, ButtonVariants};
pub use card::Card;
pub use carousel::Carousel;
pub use chart::{AreaChart, AreaFill, BarChart, Curve, LineChart, PieChart};
pub use checkbox::Checkbox;
pub use collapsible::Collapsible;
pub use combobox::Combobox;
pub use command::CommandDialog;
pub use context_menu::ContextMenu;
pub use date_picker::{
    DatePicker, DatePickerColors, DatePickerEvent, DatePickerState, DateValue, MonthGrid,
};
pub use dialog::{AlertDialog, Dialog};
pub use drawer::{DRAWER_DISMISS_FRACTION, Drawer, DrawerSide};
pub use dropdown_menu::{DropdownMenu, DropdownMenuTrigger};
pub use empty_state::EmptyState;
pub use field::{Field, FieldGroup, FieldLegend, FieldSet, Label};
pub use hover_card::{HOVER_CARD_CLOSE_DELAY, HOVER_CARD_OPEN_DELAY, HoverCard};
pub use icon::{Icon, IconName, Icons};
pub use input::{Input, InputEvent, InputState};
pub use input_group::InputGroup;
pub use input_otp::{InputOtp, InputOtpEvent, InputOtpState};
pub use item::{Item, ItemGroup, ItemSeparator, ItemVariant};
pub use kbd::{Kbd, KbdGroup};
pub use menu::{MenuReorder, MenuSelect, PopupMenu, PopupMenuColors, PopupMenuItem, SUBMENU_DELAY};
pub use menubar::{Menubar, MenubarMenu, MenubarSelect};
pub use navigation_menu::{NavigationMenu, NavigationMenuItem, NavigationMenuLink};
pub use pagination::Pagination;
pub use picker::Picker;
#[doc(hidden)]
pub use popover::claim_count;
pub use popover::{
    DismissReason, Popover, PopoverAlign, PopoverAnchor, PopoverColors, PopoverSide,
};
pub use press::{Press, focus_ring};
pub use progress::Progress;
pub use radio_group::RadioGroup;
pub use root::{FocusNext, FocusPrev, Root};
pub use scroll_area::{ScrollArea, ScrollAreaHandle, ScrollAxes};
pub use search_list::SearchList;
pub use segmented::SegmentedControl;
pub use select::Select;
pub use separator::Separator;
pub use sheet::Sheet;
pub use sidebar::{
    Sidebar, SidebarGroup, SidebarItem, SidebarState, SidebarTrigger, ToggleSidebar,
};
pub use size::{ControlSize, Disableable, Selectable, Sizable};
pub use skeleton::Skeleton;
pub use slider::{Slider, SliderEvent, SliderState};
pub use spinner::Spinner;
pub use split_view::{Pane, SplitView};
pub use switch::Switch;
pub use table::{ColumnAlign, ColumnWidth, Table, TableColumn, TableRow};
pub use tabs::{Tab, Tabs, TabsOverflow, TabsVariant};
pub use text::{TextView, TextViewStyle};
pub use theme::{ActiveTheme, Colors, Theme, ThemeMode, TypeScale};
pub use toast::{TOAST_DURATION, TOAST_DURATION_LONG, Toast, ToastKind};
pub use toggle::{Toggle, ToggleGroup, ToggleVariant};
pub use tooltip::Tooltip;

use gpui::{
    AnyWindowHandle, App, AppContext as _, Entity, Focusable, Render, Window, WindowOptions,
};

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
    accordion::init(cx);
    segmented::init(cx);
    split_view::init(cx);
    tabs::init(cx);
    radio_group::init(cx);
    toggle::init(cx);
    select::init(cx);
    date_picker::init(cx);
    modal::init(cx);
    input::init(cx);
    menu::init(cx);
    navigation_menu::init(cx);
    context_menu::init(cx);
    dropdown_menu::init(cx);
    menubar::init(cx);
    picker::init(cx);
    combobox::init(cx);
    search_list::init(cx);
    popover::init(cx);
    sidebar::init(cx);
    text::init(cx);
    table::init(cx);
    chart::init(cx);
}

pub(crate) use base::init_soon;

/// That [`init`] has run.
struct Initialized;

impl gpui::Global for Initialized {}

/// Opens a window with `build`'s view inside a [`Root`], and returns the window and the view.
///
/// The Root takes the window's first focus, so a key binding in the view's own key context
/// does nothing until something inside the view has the focus: a view with key bindings of its
/// own (a ⌘K palette, say) wants [`open_window_focusing`].
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

/// Opens a window with `build`'s view inside a [`Root`], as [`open_window`] does, with the
/// view taking the window's first focus (see [`Root::focusing`]): its key bindings work from
/// the first key, and Tab and shift-Tab still move the focus.
pub fn open_window_focusing<V: Render + Focusable>(
    options: WindowOptions,
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> gpui::Result<(AnyWindowHandle, Entity<V>)> {
    let mut built = None;
    let window = cx.open_window(options, |window, cx| {
        let view = build(window, cx);
        built = Some(view.clone());
        cx.new(|cx| Root::focusing(view, window, cx))
    })?;
    Ok((
        window.into(),
        built.expect("open_window_focusing ran its build closure"),
    ))
}
