//! What GPUI's elements can't yet say to assistive technology on their own.

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Window, accesskit,
};

/// An element, marked disabled (or not) in the accessibility tree. GPUI has no
/// `aria_disabled`; this sets it on the element's own node and draws it unchanged.
pub(crate) struct Disabled<E> {
    inner: E,
    disabled: bool,
}

impl<E: Element> Disabled<E> {
    pub(crate) fn new(inner: E, disabled: bool) -> Self {
        Self { inner, disabled }
    }
}

impl<E: Element> IntoElement for Disabled<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<E: Element> Element for Disabled<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, window, cx)
    }

    fn a11y_role(&self) -> Option<accesskit::Role> {
        self.inner.a11y_role()
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        if self.disabled {
            node.set_disabled();
        }
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}

/// An element marked as the current one of a set (`aria-current`): the page a breadcrumb or a
/// sidebar is on. GPUI has no `aria_current`; this sets it on the element's own node and draws
/// it unchanged.
pub struct Current<E> {
    inner: E,
    current: Option<accesskit::AriaCurrent>,
}

impl<E: Element> Current<E> {
    pub fn new(inner: E, current: Option<accesskit::AriaCurrent>) -> Self {
        Self { inner, current }
    }
}

impl<E: Element> IntoElement for Current<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<E: Element> Element for Current<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, window, cx)
    }

    fn a11y_role(&self) -> Option<accesskit::Role> {
        self.inner.a11y_role()
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        if let Some(current) = self.current {
            node.set_aria_current(current);
        }
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}

/// A drawn control as assistive technology gets it: what drew it, its role and its label. For
/// gpui-core's accessibility audit (`tests/a11y_labels.rs`), which records what every
/// interactive node says while a component draws; not part of the stable API.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct Audited {
    pub what: &'static str,
    pub role: gpui::Role,
    pub label: Option<gpui::SharedString>,
}

thread_local! {
    static AUDIT: std::cell::RefCell<Option<Vec<Audited>>> = const { std::cell::RefCell::new(None) };
}

/// Starts recording what interactive nodes say on this thread (until [`audit_take`]).
#[doc(hidden)]
pub fn audit_start() {
    AUDIT.with(|a| *a.borrow_mut() = Some(Vec::new()));
}

/// What was recorded since [`audit_start`], and stops recording.
#[doc(hidden)]
pub fn audit_take() -> Vec<Audited> {
    AUDIT.with(|a| a.borrow_mut().take().unwrap_or_default())
}

/// Records an interactive node's role and label while an audit runs; nothing otherwise.
#[doc(hidden)]
pub fn audit(what: &'static str, role: gpui::Role, label: Option<&gpui::SharedString>) {
    AUDIT.with(|a| {
        if let Some(list) = a.borrow_mut().as_mut() {
            list.push(Audited {
                what,
                role,
                label: label.cloned(),
            });
        }
    });
}
