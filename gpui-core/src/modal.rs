//! What a modal overlay (a dialog, a drawer) is built from: the focus kept inside it while it's
//! open, and the element that draws its scrim over the whole window in the modal layer.
//!
//! A modal draws [`ModalElement::new`] with its scrim (keyed by the modal's id, with
//! [`CONTEXT`] as its key context, so Tab and shift-Tab go round what's inside it through
//! [`cycle_focus`]); [`just_opened`] tells it when to give the keyboard to what's inside with
//! [`focus_first_in`].

use gpui::{
    App, Bounds, ContentMask, Element, ElementId, FocusHandle, GlobalElementId, InspectorElementId,
    IntoElement, KeyBinding, LayoutId, Pixels, Point, Position, Stateful, Style, Styled as _,
    Window, actions,
};

use crate::popover::{InLayer, MODAL_PRIORITY, layer};

// The namespace is the dialog's, where these began: a host's keymap naming them keeps working.
actions!(
    gpui_core_dialog,
    [
        /// Moves the focus to the next thing inside the modal, from its last to its first.
        TrapNext,
        /// Moves the focus to the previous thing inside the modal, from its first to its last.
        TrapPrev
    ]
);

/// A modal's context: Tab and shift-Tab go round what's inside it.
pub const CONTEXT: &str = "GpuiCoreModal";

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", TrapNext, Some(CONTEXT)),
        KeyBinding::new("shift-tab", TrapPrev, Some(CONTEXT)),
    ]);
}
/// Gives the keyboard to the first thing inside `container` that Tab stops on, and says
/// whether there was one; if there wasn't, `container` keeps it.
pub fn focus_first_in(container: &FocusHandle, window: &mut Window, cx: &mut App) -> bool {
    window.focus(container, cx);
    window.focus_next(cx);
    let inside = container.contains_focused(window, cx) && !container.is_focused(window);
    if !inside {
        window.focus(container, cx);
    }
    inside
}

/// Moves the keyboard to the next (or the previous) thing Tab stops on inside `container`,
/// going round from its last to its first: the window's other tab stops are passed over.
pub fn cycle_focus(container: &FocusHandle, forward: bool, window: &mut Window, cx: &mut App) {
    let start = window.focused(cx);
    let mut first = None;
    loop {
        if forward {
            window.focus_next(cx);
        } else {
            window.focus_prev(cx);
        }
        let now = window.focused(cx);
        if container.contains_focused(window, cx) && !container.is_focused(window) {
            return;
        }
        // Once round the window with nothing inside to stop on (or nothing at all).
        if now.is_none() || now == start || now == first {
            break;
        }
        if first.is_none() {
            first = now;
        }
    }
    match start {
        Some(start) => window.focus(&start, cx),
        None => window.focus(container, cx),
    }
}

/// Whether the overlay under `id` opened this frame (it was closed when last drawn).
pub fn just_opened(id: &ElementId, open: bool, window: &mut Window, cx: &mut App) -> bool {
    let key = ElementId::NamedChild(std::sync::Arc::new(id.clone()), "opened".into());
    let state = window.use_keyed_state(key, cx, |_, _| false);
    let was = *state.read(cx);
    if was != open {
        state.update(cx, |s, _| *s = open);
    }
    open && !was
}

/// Takes no room where it's declared, and draws an open modal's scrim over the whole window,
/// in the modal layer (or the layer of the overlay it's drawn from inside, if that's higher).
pub struct ModalElement {
    scrim: Option<Stateful<gpui::Div>>,
}

impl ModalElement {
    /// Draws `scrim` (none while the modal is closed). It's sized to the window here, so give
    /// it no size of its own; put the panel inside it.
    pub fn new(scrim: Option<Stateful<gpui::Div>>) -> Self {
        Self { scrim }
    }
}

impl IntoElement for ModalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ModalElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(scrim) = self.scrim.take() else {
            return;
        };
        let layer = layer(cx).max(MODAL_PRIORITY);
        let view = window.viewport_size();
        let scrim = scrim.w(view.width).h(view.height).into_any_element();
        let mut scrim = InLayer::new(layer, scrim).into_any_element();
        scrim.layout_as_root(view.map(gpui::AvailableSpace::Definite), window, cx);
        window.defer_draw(scrim, Point::default(), layer, None::<ContentMask<Pixels>>);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}
