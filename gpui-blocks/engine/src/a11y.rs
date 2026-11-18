//! What a control tells assistive technology: a role and a label, reported to gpui-core's
//! accessibility audit as it's drawn, so a test can fail on any control without a label (GPUI's
//! test windows build no accessibility tree).

use gpui::{Role, SharedString, StatefulInteractiveElement};

/// An interactive element as a control: its role and its label, set and reported together.
pub trait Control: StatefulInteractiveElement {
    /// Gives the element `role` and `label`, and reports them as `what` (the drawing module,
    /// `module_path!()`) to gpui-core's audit while one runs.
    fn control(self, what: &'static str, role: Role, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        gpui_core::audit(what, role, Some(&label));
        self.role(role).aria_label(label)
    }
}

impl<T: StatefulInteractiveElement> Control for T {}
