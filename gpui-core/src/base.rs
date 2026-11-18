//! What a component is built on: the theme, sizes, [`Press`], icons, the overlays' layers and
//! focus, menus and fields, and the window's [`Root`].
//!
//! It exists for components copied into a host's own crate (shadcn's way: the host owns the
//! component's source and changes it as it likes). A copied component reaches gpui-core only
//! through this module, as `gpui_core::base::…`, and gpui-core's own components reach it the
//! same way, as `crate::base::…`, so a copy compiles unchanged but for its paths.
//!
//! It's **semver-stable**: a breaking change to anything here (a removed item, a changed
//! signature, a different meaning) breaks copied code that can't be updated with the crate, so
//! it's a breaking change for gpui-core, made only in a new major version (or, before 1.0, a
//! new minor one) and said in its notes. Adding to it isn't breaking.
//!
//! Each item is the one defined elsewhere in the crate (most are also at its root); this module
//! only gathers what components need, and a few things they need that aren't at the root:
//! the overlays' layers and key contexts, the modal's focus trap, and [`init_soon`].

use gpui::App;

pub use crate::a11y::Current;
pub use crate::icon::{Icon, IconName};
pub use crate::init;
pub use crate::input::{Input, InputEvent, InputState, Paste};
pub use crate::menu::{MenuSelect, PopupMenu, PopupMenuItem};
pub use crate::modal::{
    CONTEXT as MODAL_CONTEXT, ModalElement, TrapNext, TrapPrev, cycle_focus, focus_first_in,
    just_opened,
};
pub use crate::picker::Picker;
pub use crate::popover::{
    CONTEXT as POPOVER_CONTEXT, Dismiss, DismissReason, InLayer, MODAL_PRIORITY,
    PRIORITY as OVERLAY_PRIORITY, Popover, PopoverAlign, PopoverColors, PopoverSide,
    layer as overlay_layer, overlay_focus,
};
pub use crate::press::{Press, focus_ring};
pub use crate::root::Root;
pub use crate::size::{ControlSize, Disableable, Selectable, Sizable};
pub use crate::theme::{ActiveTheme, Theme};

/// Runs [`init`] after this frame if it hasn't run: for a component with no state of its own to
/// run it from (keys can't be bound in the middle of a draw). A component calls it first thing
/// when it's drawn, so a host that never called [`init`] still gets the theme and gpui-core's
/// keys. It doesn't bind a copied component's own keys: the host calls that component's `init`.
pub fn init_soon(cx: &mut App) {
    if !cx.has_global::<crate::Initialized>() {
        cx.defer(init);
    }
}
