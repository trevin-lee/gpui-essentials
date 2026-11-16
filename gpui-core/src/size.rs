//! What every component shares: a size, and being disabled or selected.

/// How big a component draws. Everything but [`ControlSize::Touch`] is in rems, so it zooms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ControlSize {
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
    /// Large enough for a finger: pressable targets are at least the theme's
    /// [`touch_target`](crate::Theme::touch_target) (44 points) each way.
    Touch,
}

/// A component that comes in sizes.
pub trait Sizable: Sized {
    fn with_size(self, size: ControlSize) -> Self;

    fn xsmall(self) -> Self {
        self.with_size(ControlSize::XSmall)
    }

    fn small(self) -> Self {
        self.with_size(ControlSize::Small)
    }

    fn large(self) -> Self {
        self.with_size(ControlSize::Large)
    }

    /// At least 44 points each way, for touch screens.
    fn touch(self) -> Self {
        self.with_size(ControlSize::Touch)
    }
}

/// A component that can be turned off: it draws dimmed, takes no focus and ignores input.
pub trait Disableable: Sized {
    fn disabled(self, disabled: bool) -> Self;
}

/// A component that can be the selected one of several.
pub trait Selectable: Sized {
    fn selected(self, selected: bool) -> Self;
    fn is_selected(&self) -> bool;
}
